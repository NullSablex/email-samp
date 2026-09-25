//! The resolved settings of one account.
//!
//! Plain data: [`crate::settings`] fills it from whichever source the gamemode
//! used, and that is the only place values are parsed or validated.

/// How the session is protected.
///
/// STARTTLS is a *required* upgrade, never opportunistic: anyone who can see
/// the connection can also strip the offer, after which the client would
/// authenticate in the clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encryption {
    /// Plaintext, port 25. Only for a relay on this machine.
    None,
    /// Port 587: connect in the clear, then require the upgrade.
    StartTls,
    /// Port 465: TLS from the first byte.
    Tls,
}

impl Encryption {
    /// The port to use when the configuration did not set one.
    pub fn default_port(self) -> u16 {
        match self {
            Self::None => 25,
            Self::StartTls => 587,
            Self::Tls => 465,
        }
    }

    /// How `email_status` names the mode.
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "PLAINTEXT",
            Self::StartTls => "STARTTLS",
            Self::Tls => "TLS",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmailOptions {
    /// `None` follows [`Encryption::default_port`], so changing the mode
    /// moves the port with it.
    pub port: Option<u16>,
    pub encryption: Encryption,
    /// Default `From:`, overridable per message with `email_set_from`.
    pub from_address: String,
    pub from_name: String,
    /// Name announced in EHLO. Empty means the machine's own hostname.
    pub helo_name: String,
    pub timeout_secs: u64,
    /// Maximum simultaneous SMTP sessions held open for reuse.
    pub pool_size: u32,
    /// Extra root certificate (PEM), for a relay with a private CA.
    pub tls_ca: Option<String>,
    /// False accepts any certificate, which hands the password to an active
    /// attacker. `connect` warns on every use.
    pub tls_verify_cert: bool,
    /// Extra attempts after a *temporary* failure. A permanent one is never
    /// retried: resending to a rejected recipient only gets the sender
    /// flagged.
    pub retries: u32,
    /// Messages per minute; 0 is unlimited. Providers throttle senders that
    /// burst.
    pub rate_limit: u32,
    /// How Pawn's 8-bit strings are read: SA-MP has no notion of UTF-8, so a
    /// nickname with an accent is a byte in the server's code page. Reading it
    /// as the wrong one is what turns "João" into "Jo?o" in the mail.
    ///
    /// Any label the WHATWG standard knows works — the server picks its own,
    /// instead of the plugin compiling a short list in. Process-wide.
    pub charset: &'static encoding_rs::Encoding,
    /// Allows sending the password over an unencrypted connection to a host
    /// that is not this machine. Off, and such a setup is refused outright.
    pub allow_plaintext_auth: bool,
    /// Writes each message to `logs/dry-run/` instead of sending it.
    pub dry_run: bool,
    /// Messages that may wait at once, past which a send fails with
    /// `EMAIL_ERROR_QUEUE_FULL` rather than growing the queue.
    pub queue_limit: u32,
}

impl Default for EmailOptions {
    fn default() -> Self {
        Self {
            port: None,
            encryption: Encryption::StartTls,
            from_address: String::new(),
            from_name: String::new(),
            helo_name: String::new(),
            timeout_secs: 30,
            pool_size: 4,
            tls_ca: None,
            tls_verify_cert: true,
            retries: 2,
            rate_limit: 0,
            queue_limit: 1000,
            charset: encoding_rs::WINDOWS_1252,
            allow_plaintext_auth: false,
            dry_run: false,
        }
    }
}

impl EmailOptions {
    /// The port actually dialled.
    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or_else(|| self.encryption.default_port())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_submission_over_required_starttls() {
        let opts = EmailOptions::default();
        assert_eq!(opts.encryption, Encryption::StartTls);
        assert_eq!(opts.effective_port(), 587);
        assert!(opts.tls_verify_cert);
    }

    #[test]
    fn the_default_port_follows_the_encryption_mode() {
        for (mode, port) in [
            (Encryption::None, 25),
            (Encryption::StartTls, 587),
            (Encryption::Tls, 465),
        ] {
            let opts = EmailOptions {
                encryption: mode,
                ..EmailOptions::default()
            };
            assert_eq!(opts.effective_port(), port);
        }
    }

    #[test]
    fn the_default_charset_is_what_sa_mp_uses() {
        assert_eq!(EmailOptions::default().charset.name(), "windows-1252");
    }

    #[test]
    fn an_explicit_port_wins_over_the_mode_default() {
        let opts = EmailOptions {
            port: Some(2525),
            encryption: Encryption::Tls,
            ..EmailOptions::default()
        };
        assert_eq!(opts.effective_port(), 2525);
    }
}
