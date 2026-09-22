use samp::native;
use samp::prelude::*;

use crate::plugin::EmailPlugin;

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
        dest.write_str(dest_len, &message)?;
        Ok(true)
    }
}
