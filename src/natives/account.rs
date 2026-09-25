use samp::args::Args;
use samp::native;
use samp::prelude::*;

use crate::account::Warning;
use crate::logger::Logger;
use crate::plugin::EmailPlugin;
use crate::settings::{self, Settings, Source};

/// `email_stats(EMAIL_EVERY_ACCOUNT, ...)`: the whole queue, not one account.
const EVERY_ACCOUNT: i32 = -1;

impl EmailPlugin {
    /// `email_setup(const source[] = "")` -> account id, or 0.
    #[native(name = "email_setup")]
    pub fn email_setup(&mut self, _amx: &Amx, source: &AmxString) -> i32 {
        match settings::resolve(&source.to_string()) {
            Ok((settings, origin)) => self.open_account(&settings, &origin),
            Err(fail) => {
                self.report(0, fail, "Setup");
                0
            }
        }
    }

    /// `email_connect(const host[], const user[], const password[], const settings[] = "")`
    #[native(name = "email_connect")]
    pub fn email_connect(
        &mut self,
        _amx: &Amx,
        host: &AmxString,
        user: &AmxString,
        password: &AmxString,
        inline: &AmxString,
    ) -> i32 {
        let resolved = settings::from_connect(
            &host.to_string(),
            &user.to_string(),
            &password.to_string(),
            &inline.to_string(),
        );
        match resolved {
            Ok(settings) => self.open_account(&settings, &Source::Code),
            Err(fail) => {
                self.report(0, fail, "Connect");
                0
            }
        }
    }

    /// `email_close(account = 0)` - 0 closes the default account.
    #[native(name = "email_close")]
    pub fn email_close(&mut self, _amx: &Amx, account_id: i32) -> bool {
        let account_id = self.resolve_account(account_id);

        if !self.accounts.close(account_id) {
            Logger::warn("Account not found.");
            return false;
        }

        self.messages.destroy_by_account(account_id);
        if self.default_account == account_id {
            self.default_account = 0;
        }

        Logger::info(&format!("Account {account_id} closed."));
        true
    }

    /// `email_is_account(account = 0)` - whether the handle is still open.
    #[native(name = "email_is_account")]
    pub fn email_is_account(&mut self, _amx: &Amx, account_id: i32) -> bool {
        self.accounts.exists(self.resolve_account(account_id))
    }

    /// `email_status(account, dest[], len = sizeof dest)`
    #[native(name = "email_status")]
    pub fn email_status(
        &mut self,
        _amx: &Amx,
        account_id: i32,
        dest: UnsizedBuffer,
        dest_len: usize,
    ) -> AmxResult<bool> {
        let Some(account) = self.accounts.get(self.resolve_account(account_id)) else {
            return Ok(false);
        };

        let status = format!(
            "{}:{} {}",
            account.host,
            account.port,
            account.encryption.label()
        );
        dest.write_str(dest_len, &status)?;
        Ok(true)
    }

    /// `email_stats(account = 0, &queued = 0, &sent = 0, &failed = 0)`
    #[native(name = "email_stats")]
    pub fn email_stats(
        &mut self,
        _amx: &Amx,
        account_id: i32,
        queued: Ref<i32>,
        sent: Ref<i32>,
        failed: Ref<i32>,
    ) -> bool {
        // -1 is the whole queue: every account added up, which is what a
        // server with a second sender wants to see.
        let counts = if account_id == EVERY_ACCOUNT {
            Some(self.accounts.totals())
        } else {
            self.accounts
                .handle(self.resolve_account(account_id))
                .map(|account| account.stats.get())
        };

        let Some((q, s, f)) = counts else {
            return false;
        };
        for (mut out, value) in [(queued, q), (sent, s), (failed, f)] {
            *out = i32::try_from(value).unwrap_or(i32::MAX);
        }
        true
    }

    /// `email_test(account = 0, const callback[] = "", const format[] = "", {Float,_}:...)`
    #[native(name = "email_test", raw)]
    pub fn email_test(&mut self, _amx: &Amx, mut args: Args) -> bool {
        let account_id = self.resolve_account(args.next_arg::<i32>().unwrap_or(0));
        let callback = super::read_callback_args(&mut args);
        self.queue(account_id, None, callback)
    }

    /// Shared tail of both connect paths: build the account, report the
    fn open_account(&mut self, settings: &Settings, origin: &Source) -> i32 {
        let (id, warnings) = match self.accounts.connect(settings) {
            Ok(result) => result,
            Err(fail) => {
                self.report(0, fail, "Connect");
                return 0;
            }
        };

        // These go to the console, not just the file: both describe a setup
        // that looks like it works and quietly is not secure, which is exactly
        // what nobody finds in a log.
        let host = &settings.host;
        for warning in warnings {
            Logger::warn(&match warning {
                Warning::PlaintextCredentials => format!(
                    "Account {id} sends credentials over an unencrypted connection to '{host}'. \
                     Only do this for a relay on this machine."
                ),
                Warning::VerificationDisabled => format!(
                    "Account {id} does not verify the relay's TLS certificate. The connection \
                     can be intercepted; use the tls_ca setting instead."
                ),
            });
        }

        // Process-wide, because the AMX conversion is: the last account
        // opened decides, which only matters to a server whose accounts
        // disagree about its own gamemode's encoding.
        samp::encoding::set_default_encoding(settings.options.charset);

        self.register_account(id);

        // Naming the source matters once discovery can pick between the
        // environment and a file: "why is it using the wrong relay" is
        // otherwise guesswork.
        Logger::info(&format!(
            "Account {id} configured from {}.",
            origin.describe()
        ));
        id
    }
}
