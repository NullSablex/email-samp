//! SMTP accounts: a relay, its credentials and the pooled transport.
//!
//! [`SmtpTransport`] owns a connection pool that workers share through
//! [`AccountHandle`], so a session is authenticated once and reused —
//! providers rate-limit on connections.
//!
//! TLS verification uses the webpki bundle compiled into the plugin, not the
//! system store: an OS store would mean binding OpenSSL, which the pure-Rust
//! i686/MSVC cross-build cannot afford. Public relays work unconfigured; a
//! private CA needs the `tls_ca` setting.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Certificate, Tls, TlsParameters};
use lettre::transport::smtp::extension::ClientId;
use lettre::transport::smtp::{PoolConfig, SmtpTransport};

use crate::error::{EmailError, ErrorState, Fail};
use crate::options::{EmailOptions, Encryption};
use crate::settings::Settings;

pub struct Account {
    pub shared: AccountHandle,
    pub host: String,
    pub port: u16,
    pub encryption: Encryption,
    pub error: ErrorState,
}

/// What a worker thread needs to send, shared with the main thread.
///
/// Closing an account drops the main thread's reference only: a message
/// already queued keeps its own, so it still goes out.
pub type AccountHandle = Arc<Shared>;

pub struct Shared {
    pub transport: SmtpTransport,
    /// Default `From:` for messages on this account.
    pub from: Mailbox,
    pub retries: u32,
    pub queue_limit: u32,
    /// Nothing is sent: messages are written to `logs/dry-run/`.
    pub dry_run: bool,
    pub gate: Mutex<Gate>,
    pub stats: Stats,
}

/// Counters behind `email_stats`. Updated by the workers, read by the main
/// thread; `Relaxed` is enough because nothing is ordered against them.
#[derive(Default)]
pub struct Stats {
    pub queued: AtomicU32,
    pub sent: AtomicU32,
    pub failed: AtomicU32,
}

impl Stats {
    pub fn get(&self) -> (u32, u32, u32) {
        (
            self.queued.load(Ordering::Relaxed),
            self.sent.load(Ordering::Relaxed),
            self.failed.load(Ordering::Relaxed),
        )
    }
}

/// When an account may next talk to its relay. Read by the scheduler, so a
/// rate-limited or paused account holds back only its own messages.
pub struct Gate {
    /// `60 s / rate_limit`, or `None` when unlimited.
    interval: Option<Duration>,
    next: Instant,
    /// Connection failures in a row, which set how long the account pauses.
    failures: u32,
}

/// Longest an account pauses after repeated connection failures.
const MAX_PAUSE: Duration = Duration::from_secs(300);

impl Gate {
    pub fn new(rate_per_minute: u32) -> Self {
        Self {
            interval: (rate_per_minute > 0).then(|| Duration::from_secs(60) / rate_per_minute),
            next: Instant::now(),
            failures: 0,
        }
    }

    /// Earliest moment the account may send.
    pub fn ready_at(&self) -> Instant {
        self.next
    }

    /// Spends one send slot. Evenly spaced rather than in bursts, because
    /// providers look at short windows: a full minute's allowance in the first
    /// second is exactly what they throttle.
    pub fn take(&mut self, now: Instant) {
        if let Some(interval) = self.interval {
            self.next = self.next.max(now) + interval;
        }
    }

    /// The relay could not be reached. Pausing the whole account — instead of
    /// letting each queued message burn its own retries against a dead relay
    /// — is what makes an outage cost one probe per pause, not one per mail.
    /// The pause doubles with each failure in a row, up to [`MAX_PAUSE`].
    pub fn pause(&mut self, now: Instant, base: Duration) {
        let pause = base
            .saturating_mul(1 << self.failures.min(16))
            .min(MAX_PAUSE);
        self.failures = self.failures.saturating_add(1);
        self.next = self.next.max(now + pause);
    }

    pub fn recovered(&mut self) {
        self.failures = 0;
    }

    #[cfg(test)]
    pub fn failures(&self) -> u32 {
        self.failures
    }
}

pub struct AccountManager {
    accounts: HashMap<i32, Account>,
    next_id: i32,
    /// Errors from before an account existed — a bad config file, a
    /// malformed sender address. Reported as `email_errno(0)`.
    pub global_error: ErrorState,
}

/// Warnings raised while building a transport. Returned rather than logged
/// directly so [`AccountManager`] stays free of the logger, which keeps it
/// testable.
#[derive(Debug, PartialEq, Eq)]
pub enum Warning {
    /// Credentials would cross the network in the clear.
    PlaintextCredentials,
    /// Certificate verification is off.
    VerificationDisabled,
}

impl AccountManager {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            next_id: 1,
            global_error: ErrorState::ok(),
        }
    }

    /// Builds the transport and registers the account.
    ///
    /// Nothing is dialled here: `SmtpTransport` connects lazily on first send,
    /// and doing a blocking handshake inside `OnGameModeInit` would stall the
    /// server for as long as the relay takes to answer. `email_test` exists
    /// for gamemodes that want to verify the credentials up front, and it goes
    /// through a worker thread.
    pub fn connect(&mut self, settings: &Settings) -> Result<(i32, Vec<Warning>), Fail> {
        let Settings {
            host,
            user,
            password,
            options: opts,
        } = settings;

        // The account must be able to say who a message is from. Falling back
        // to the SMTP username is what every gamemode would write by hand, and
        // for most providers the username *is* the mailbox.
        let from_address = if opts.from_address.is_empty() {
            user
        } else {
            &opts.from_address
        };
        let from = crate::address::parse_mailbox(from_address, &opts.from_name).map_err(
            |(code, message)| {
                (
                    code,
                    format!("the sender identity is unusable ({message}); set the 'from' setting"),
                )
            },
        )?;

        let (transport, warnings) = build_transport(host, user, password, opts)?;

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.accounts.insert(
            id,
            Account {
                shared: Arc::new(Shared {
                    transport,
                    from,
                    retries: opts.retries,
                    dry_run: opts.dry_run,
                    queue_limit: opts.queue_limit,
                    gate: Mutex::new(Gate::new(opts.rate_limit)),
                    stats: Stats::default(),
                }),
                host: host.clone(),
                port: opts.effective_port(),
                encryption: opts.encryption,
                error: ErrorState::ok(),
            },
        );

        Ok((id, warnings))
    }

    pub fn get(&self, id: i32) -> Option<&Account> {
        self.accounts.get(&id)
    }

    /// A reference for a worker thread.
    pub fn handle(&self, id: i32) -> Option<AccountHandle> {
        self.accounts.get(&id).map(|a| a.shared.clone())
    }

    pub fn close(&mut self, id: i32) -> bool {
        self.accounts.remove(&id).is_some()
    }

    pub fn exists(&self, id: i32) -> bool {
        self.accounts.contains_key(&id)
    }

    /// Records a failure against its account *and* in the global slot.
    ///
    /// The global slot is what `email_errno()` reads: the last thing that went
    /// wrong, whichever account it belonged to. Without it a gamemode could
    /// not see a failed `email_setup`/`email_connect` at all — there is no
    /// account to ask yet — and after a failed send on the default account it
    /// would read a slot nobody had written.
    pub fn set_error(&mut self, id: i32, state: ErrorState) {
        if let Some(account) = self.accounts.get_mut(&id) {
            account.error = state.clone();
        }
        self.global_error = state;
    }

    /// The three counters added up across every open account, for a server
    /// that runs more than one.
    pub fn totals(&self) -> (u32, u32, u32) {
        self.accounts.values().fold((0, 0, 0), |sum, account| {
            let (queued, sent, failed) = account.shared.stats.get();
            (
                sum.0.saturating_add(queued),
                sum.1.saturating_add(sent),
                sum.2.saturating_add(failed),
            )
        })
    }

    pub fn get_error(&self, id: i32) -> &ErrorState {
        self.accounts
            .get(&id)
            .map(|a| &a.error)
            .unwrap_or(&self.global_error)
    }

    pub fn clear_error(&mut self, id: i32) {
        if let Some(account) = self.accounts.get_mut(&id) {
            account.error = ErrorState::ok();
        }
    }
}

/// Assembles the [`SmtpTransport`] for a set of options.
fn build_transport(
    host: &str,
    user: &str,
    password: &str,
    opts: &EmailOptions,
) -> Result<(SmtpTransport, Vec<Warning>), Fail> {
    let mut warnings = Vec::new();
    let authenticating = !user.is_empty() || !password.is_empty();

    let tls = match opts.encryption {
        Encryption::None => {
            // A relay on this machine is a legitimate setup. The same
            // configuration pointed at a remote host hands the SMTP password
            // to anyone on the path, so it takes an explicit opt-in rather
            // than a warning nobody reads.
            if authenticating && !is_local(host) && !opts.allow_plaintext_auth {
                return Err(EmailError::ConfigFailed.because(format!(
                    "refusing to send the password to '{host}' over an unencrypted \
                     connection: use encryption=starttls, or set \
                     allow_plaintext_auth=1 if the network really is trusted"
                )));
            }
            if authenticating {
                warnings.push(Warning::PlaintextCredentials);
            }
            Tls::None
        }
        // `Required`, never `Opportunistic`: an attacker who can see the
        // connection can also strip the STARTTLS capability from the server's
        // EHLO reply, after which an opportunistic client cheerfully
        // authenticates in the clear. Requiring the upgrade turns that attack
        // into a failed send.
        Encryption::StartTls => Tls::Required(tls_parameters(host, opts, &mut warnings)?),
        Encryption::Tls => Tls::Wrapper(tls_parameters(host, opts, &mut warnings)?),
    };

    let mut builder = SmtpTransport::builder_dangerous(host)
        .port(opts.effective_port())
        .tls(tls)
        .timeout(Some(Duration::from_secs(opts.timeout_secs)))
        .pool_config(PoolConfig::new().max_size(opts.pool_size));

    if !opts.helo_name.is_empty() {
        builder = builder.hello_name(ClientId::Domain(opts.helo_name.clone()));
    }

    if authenticating {
        builder = builder.credentials(Credentials::new(user.to_string(), password.to_string()));
    }

    Ok((builder.build(), warnings))
}

fn tls_parameters(
    host: &str,
    opts: &EmailOptions,
    warnings: &mut Vec<Warning>,
) -> Result<TlsParameters, Fail> {
    let mut builder = TlsParameters::builder(host.to_string());

    if let Some(path) = &opts.tls_ca {
        let pem = std::fs::read(path).map_err(|e| {
            EmailError::ConfigFailed.because(format!("could not read the CA file '{path}': {e}"))
        })?;
        let certificate = Certificate::from_pem(&pem).map_err(|e| {
            EmailError::ConfigFailed
                .because(format!("'{path}' is not a valid PEM certificate: {e}"))
        })?;
        builder = builder.add_root_certificate(certificate);
    }

    if !opts.tls_verify_cert {
        warnings.push(Warning::VerificationDisabled);
        // Hostname checking goes with it: leaving it on while accepting any
        // certificate is a half-measure that protects nothing, and having it
        // reject a connection the operator already opted into would just be
        // confusing.
        builder = builder
            .dangerous_accept_invalid_certs(true)
            .dangerous_accept_invalid_hostnames(true);
    }

    builder
        .build()
        .map_err(|e| EmailError::ConfigFailed.because(format!("could not configure TLS: {e}")))
}

/// Whether the relay runs on this machine, where an unencrypted session never
/// reaches a network.
fn is_local(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host == "::1"
        || host
            .strip_prefix("127.")
            .is_some_and(|rest| !rest.is_empty())
}

/// Maps a `lettre` SMTP error onto a plugin error code.
///
/// Anything without an SMTP reply code never got as far as a conversation
/// with the relay — refused, unreachable, timed out, TLS failed — and is a
/// connection failure. With a reply code it is either authentication or a
/// refused message.
pub fn classify(error: &lettre::transport::smtp::Error) -> EmailError {
    let Some(code) = error.status() else {
        return EmailError::ConnectionFailed;
    };

    // 535 is the common one; 530 is "authentication required", 534/538 are
    // mechanism complaints, 454 is a temporary auth failure.
    match code.to_string().as_str() {
        "535" | "534" | "530" | "538" | "454" => EmailError::AuthFailed,
        _ => EmailError::SendFailed,
    }
}

/// Whether trying again later can succeed.
///
/// A 4xx reply says so explicitly. So does a network-level failure (refused,
/// reset, timed out): the relay may be restarting. A 5xx, a TLS failure or a
/// client-side error will fail identically on every attempt.
pub fn is_retryable(error: &lettre::transport::smtp::Error) -> bool {
    if error.is_transient() {
        return true;
    }
    error.status().is_none()
        && !error.is_tls()
        && !error.is_client()
        && !error.is_response()
        && !error.is_transport_shutdown()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> EmailOptions {
        EmailOptions {
            from_address: "noreply@example.com".into(),
            ..EmailOptions::default()
        }
    }

    fn settings(host: &str, user: &str, options: EmailOptions) -> Settings {
        Settings {
            host: host.into(),
            user: user.into(),
            // No user means no login at all, as for a local relay.
            password: if user.is_empty() {
                String::new()
            } else {
                "pass".into()
            },
            options,
        }
    }

    #[test]
    fn a_starttls_account_connects_and_gets_handle_one() {
        let mut mgr = AccountManager::new();
        let (id, warnings) = mgr
            .connect(&settings("smtp.example.com", "user", options()))
            .expect("builds");
        assert_eq!(id, 1);
        assert!(warnings.is_empty());

        let account = mgr.get(id).expect("registered");
        assert_eq!(account.port, 587);
        assert_eq!(account.shared.from.email.to_string(), "noreply@example.com");
    }

    #[test]
    fn the_username_becomes_the_sender_when_no_from_is_configured() {
        let mut mgr = AccountManager::new();
        let (id, _) = mgr
            .connect(&settings(
                "smtp.example.com",
                "bot@example.com",
                EmailOptions::default(),
            ))
            .expect("builds");
        assert_eq!(
            mgr.get(id)
                .expect("registered")
                .shared
                .from
                .email
                .to_string(),
            "bot@example.com"
        );
    }

    #[test]
    fn a_username_that_is_not_an_address_fails_with_a_hint() {
        let mut mgr = AccountManager::new();
        let Err((code, message)) = mgr.connect(&settings(
            "smtp.example.com",
            "not-an-address",
            EmailOptions::default(),
        )) else {
            panic!("expected an unusable sender identity to fail the connect");
        };
        assert_eq!(code, EmailError::InvalidAddress);
        assert!(message.contains("'from'"));
    }

    #[test]
    fn plaintext_with_credentials_warns() {
        let opts = EmailOptions {
            encryption: Encryption::None,
            ..options()
        };
        let mut mgr = AccountManager::new();
        let (_, warnings) = mgr
            .connect(&settings("localhost", "user", opts.clone()))
            .expect("builds");
        assert_eq!(warnings, vec![Warning::PlaintextCredentials]);
    }

    #[test]
    fn plaintext_credentials_to_a_remote_host_are_refused() {
        let opts = EmailOptions {
            encryption: Encryption::None,
            ..options()
        };
        let mut mgr = AccountManager::new();
        let Err((code, message)) =
            mgr.connect(&settings("relay.example.com", "user", opts.clone()))
        else {
            panic!("expected plaintext credentials to a remote host to be refused");
        };
        assert_eq!(code, EmailError::ConfigFailed);
        assert!(message.contains("allow_plaintext_auth"));

        // On this machine it is a legitimate setup, with a warning.
        for host in ["localhost", "127.0.0.1", "::1"] {
            let (_, warnings) = mgr
                .connect(&settings(host, "user", opts.clone()))
                .expect("a local relay is allowed");
            assert_eq!(warnings, vec![Warning::PlaintextCredentials], "for {host}");
        }

        // Or with the explicit opt-in.
        let allowed = EmailOptions {
            allow_plaintext_auth: true,
            ..opts
        };
        let (_, warnings) = mgr
            .connect(&settings("relay.example.com", "user", allowed))
            .expect("opted in");
        assert_eq!(warnings, vec![Warning::PlaintextCredentials]);
    }

    #[test]
    fn plaintext_without_credentials_is_silent() {
        let opts = EmailOptions {
            encryption: Encryption::None,
            ..options()
        };
        let mut mgr = AccountManager::new();
        let (_, warnings) = mgr
            .connect(&settings("localhost", "", opts.clone()))
            .expect("builds");
        assert!(warnings.is_empty());
    }

    #[test]
    fn disabling_verification_warns() {
        let opts = EmailOptions {
            tls_verify_cert: false,
            ..options()
        };
        let mut mgr = AccountManager::new();
        let (_, warnings) = mgr
            .connect(&settings("smtp.example.com", "u", opts.clone()))
            .expect("builds");
        assert_eq!(warnings, vec![Warning::VerificationDisabled]);
    }

    #[test]
    fn an_unreadable_ca_file_fails_the_connect() {
        let opts = EmailOptions {
            tls_ca: Some("/definitely/not/here.pem".into()),
            ..options()
        };
        let mut mgr = AccountManager::new();
        let Err((code, message)) = mgr.connect(&settings("smtp.example.com", "u", opts.clone()))
        else {
            panic!("expected a missing CA file to fail the connect");
        };
        assert_eq!(code, EmailError::ConfigFailed);
        assert!(message.contains("here.pem"));
    }

    #[test]
    fn the_gate_spaces_slots_evenly() {
        let mut gate = Gate::new(60);
        let now = Instant::now();
        assert!(gate.ready_at() <= now);
        gate.take(now);
        assert_eq!(gate.ready_at(), now + Duration::from_secs(1));

        let mut unlimited = Gate::new(0);
        let now = Instant::now();
        unlimited.take(now);
        assert!(unlimited.ready_at() <= now);
    }

    #[test]
    fn repeated_connection_failures_pause_longer_up_to_a_cap() {
        let base = Duration::from_secs(5);
        let mut gate = Gate::new(0);
        let now = Instant::now();
        gate.pause(now, base);
        assert_eq!(gate.ready_at(), now + base);
        gate.pause(now, base);
        assert_eq!(gate.ready_at(), now + base * 2);
        for _ in 0..20 {
            gate.pause(now, base);
        }
        assert_eq!(gate.ready_at(), now + MAX_PAUSE);
        gate.recovered();
        gate.pause(now, base);
        // Recovery resets the doubling, but never shortens a pause in force.
        assert_eq!(gate.ready_at(), now + MAX_PAUSE);
    }

    #[test]
    fn account_limits_come_from_the_options() {
        let opts = EmailOptions {
            retries: 5,
            queue_limit: 7,
            rate_limit: 30,
            ..options()
        };
        let mut mgr = AccountManager::new();
        let (id, _) = mgr
            .connect(&settings("smtp.example.com", "u", opts))
            .expect("builds");
        let handle = mgr.handle(id).expect("open");
        assert_eq!(handle.retries, 5);
        assert_eq!(handle.queue_limit, 7);
        assert!(handle.gate.lock().expect("gate").interval.is_some());
        assert_eq!(handle.stats.get(), (0, 0, 0));
    }

    #[test]
    fn closing_an_account_removes_it() {
        let mut mgr = AccountManager::new();
        let (id, _) = mgr
            .connect(&settings("smtp.example.com", "u@e.com", options()))
            .expect("builds");
        assert!(mgr.exists(id));
        assert!(mgr.handle(id).is_some());
        assert!(mgr.close(id));
        assert!(!mgr.close(id));
        assert!(mgr.handle(id).is_none());
    }

    #[test]
    fn an_error_for_an_unknown_account_lands_in_the_global_slot() {
        let mut mgr = AccountManager::new();
        mgr.set_error(42, ErrorState::new(EmailError::SendFailed, "gone"));
        assert_eq!(mgr.get_error(0).code, EmailError::SendFailed);
        assert_eq!(mgr.get_error(42).code, EmailError::SendFailed);
    }

    #[test]
    fn totals_add_up_every_account() {
        let mut mgr = AccountManager::new();
        let (a, _) = mgr
            .connect(&settings("a.example.com", "u@e.com", options()))
            .expect("builds");
        let (b, _) = mgr
            .connect(&settings("b.example.com", "u@e.com", options()))
            .expect("builds");

        for (id, sent) in [(a, 2u32), (b, 3u32)] {
            let account = mgr.handle(id).expect("open");
            account
                .stats
                .queued
                .store(1, std::sync::atomic::Ordering::Relaxed);
            account
                .stats
                .sent
                .store(sent, std::sync::atomic::Ordering::Relaxed);
        }
        assert_eq!(mgr.totals(), (2, 5, 0));

        mgr.close(a);
        assert_eq!(mgr.totals(), (1, 3, 0));
    }

    #[test]
    fn the_global_slot_holds_the_last_failure_whatever_the_account() {
        let mut mgr = AccountManager::new();
        let (a, _) = mgr
            .connect(&settings("a.example.com", "u@e.com", options()))
            .expect("builds");

        // A failure before any account exists — a bad config file — has no
        // account to land on, and must still be readable.
        mgr.set_error(0, ErrorState::new(EmailError::ConfigFailed, "no host"));
        assert_eq!(mgr.get_error(0).code, EmailError::ConfigFailed);

        // One on a real account lands in both places.
        mgr.set_error(a, ErrorState::new(EmailError::AuthFailed, "535"));
        assert_eq!(mgr.get_error(a).code, EmailError::AuthFailed);
        assert_eq!(mgr.get_error(0).code, EmailError::AuthFailed);
    }

    #[test]
    fn per_account_errors_are_independent() {
        let mut mgr = AccountManager::new();
        let (a, _) = mgr
            .connect(&settings("a.example.com", "u@e.com", options()))
            .expect("builds");
        let (b, _) = mgr
            .connect(&settings("b.example.com", "u@e.com", options()))
            .expect("builds");

        mgr.set_error(a, ErrorState::new(EmailError::AuthFailed, "535"));
        assert_eq!(mgr.get_error(a).code, EmailError::AuthFailed);
        assert_eq!(mgr.get_error(b).code, EmailError::Ok);

        mgr.clear_error(a);
        assert_eq!(mgr.get_error(a).code, EmailError::Ok);
    }
}
