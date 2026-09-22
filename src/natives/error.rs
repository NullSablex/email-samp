use samp::native;
use samp::prelude::*;

use crate::plugin::EmailPlugin;

impl EmailPlugin {
    /// `email_errno(account = 0)` — the `EMAIL_ERROR_*` code of the last
    #[native(name = "email_errno")]
    pub fn email_errno(&mut self, _amx: &Amx, account_id: i32) -> i32 {
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
        let message = self.accounts.get_error(account_id).message.clone();
        dest.write_str(dest_len, &message)?;
        Ok(true)
    }
}
