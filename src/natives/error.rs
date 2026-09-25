use samp::native;
use samp::prelude::*;

use crate::logger::Logger;
use crate::plugin::EmailPlugin;

/// Writes `text` into a Pawn buffer, saying so when the server's code page
/// cannot represent it.
///
/// The conversion is lossy by nature — a Cyrillic relay reply on a
/// Windows-1252 server has nowhere to go — but arriving as `?????` with
/// nothing said anywhere is how an hour gets spent on the wrong question.
fn write_checked(dest: UnsizedBuffer, dest_len: usize, text: &str, what: &str) -> AmxResult<bool> {
    if !dest.write_str_checked(dest_len, text)? {
        Logger::warn(&format!(
            "The {what} has characters this server's charset cannot write; \
             they arrive as '?'. The full text is in logs/email.log."
        ));
    }
    Ok(true)
}

impl EmailPlugin {
    /// `email_errno(account = 0)` — the `EMAIL_ERROR_*` code of the last
    /// failure. `0` is the default account, or the global slot when no
    /// account is open, which is where failures from before one existed go.
    #[native(name = "email_errno")]
    pub fn email_errno(&mut self, _amx: &Amx, account_id: i32) -> i32 {
        let account_id = self.resolve_account(account_id);
        self.accounts.get_error(account_id).code.code()
    }

    /// `email_error(account, dest[], len = sizeof dest)` — the message behind
    #[native(name = "email_error")]
    pub fn email_error(
        &mut self,
        _amx: &Amx,
        account_id: i32,
        dest: UnsizedBuffer,
        dest_len: usize,
    ) -> AmxResult<bool> {
        let account_id = self.resolve_account(account_id);
        let message = self.accounts.get_error(account_id).message.clone();
        write_checked(dest, dest_len, &message, "error message")
    }
}
