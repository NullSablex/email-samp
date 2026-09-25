use std::time::Duration;

use samp::plugin::TickContext;
use samp::prelude::*;

use crate::account::AccountManager;
use crate::callback::{self, CallbackInfo, CallbackParam, Script};
use crate::error::{EmailError, ErrorState, Fail};
use crate::logger::Logger;
use crate::message::MessageManager;
use crate::sender::{Job, Priority, SendJob, SendManager};
use crate::template::TemplateCache;

/// How long shutdown waits for queued mail to leave.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

pub struct EmailPlugin {
    pub accounts: AccountManager,
    pub messages: MessageManager,
    pub templates: TemplateCache,
    pub sender: SendManager,
    pub amx_list: Vec<Script>,
    /// The account a native given `0` uses: the first one opened. It is what
    /// lets every call site drop the argument.
    pub default_account: i32,
}

impl EmailPlugin {
    pub fn new() -> Self {
        Logger::init();

        Self {
            accounts: AccountManager::new(),
            messages: MessageManager::new(),
            templates: TemplateCache::new(),
            sender: SendManager::new(),
            amx_list: Vec::new(),
            default_account: 0,
        }
    }

    /// Maps `0` onto the default account. `0` back means there is none, which
    /// callers already treat as "unknown account".
    pub fn resolve_account(&self, account_id: i32) -> i32 {
        if account_id == 0 {
            self.default_account
        } else {
            account_id
        }
    }

    /// First account opened wins the default, so a filterscript opening its
    /// own does not redirect the gamemode's mail.
    pub fn register_account(&mut self, account_id: i32) {
        if self.default_account == 0 {
            self.default_account = account_id;
        }
    }

    /// Logs a failure and records it where `email_errno(account)` reads it:
    /// the console gets `what` and a code, the file the detail (a recipient
    /// address, the relay's words). Returns false, so a native can
    /// `return self.report(..)`.
    pub fn report(&mut self, account_id: i32, (code, detail): Fail, what: &str) -> bool {
        Logger::error_detail(
            &format!(
                "{what} failed (error {}). See logs/email.log for details.",
                code.code()
            ),
            &format!("{what} failed: {detail}"),
        );
        self.accounts
            .set_error(account_id, ErrorState::new(code, detail));
        false
    }

    /// The one way into the send queue, shared by `email_send`,
    /// `email_send_to` and `email_test`.
    ///
    /// `message` is the draft to send, or `None` for a connection test. A
    /// refused send (closed account, full queue) leaves the handle alive.
    pub fn queue(&mut self, account_id: i32, message: Option<i32>, callback: CallbackInfo) -> bool {
        let Some(account) = self.accounts.handle(account_id) else {
            let fail = EmailError::InvalidAccount.because("the account is not open");
            return self.report(account_id, fail, "Send");
        };
        // A connection test is someone at the console waiting for an answer.
        let priority = match message {
            None => Priority::High,
            Some(id) => match self.messages.get(id) {
                Some(draft) => draft.priority,
                None => return self.invalid_message(id),
            },
        };
        if let Err(fail) = SendManager::check_room(&account, priority) {
            return self.report(account_id, fail, "Send");
        }

        let job = match message.and_then(|id| self.messages.take(id)) {
            Some(draft) => Job::Send(Box::new(draft)),
            None => Job::Test,
        };

        let submitted = self.sender.submit(SendJob {
            account_id,
            account,
            job,
            priority,
            callback,
        });
        match submitted {
            Ok(()) => true,
            Err(fail) => self.report(account_id, fail, "Send"),
        }
    }

    /// Goes to the global slot: with no live draft there is no account to
    /// attribute it to.
    pub fn invalid_message(&mut self, message_id: i32) -> bool {
        self.accounts.global_error = ErrorState::new(
            EmailError::InvalidMessage,
            format!("Invalid message handle ({message_id})."),
        );
        false
    }

    /// Turns finished sends into Pawn callbacks. Main thread only.
    pub fn process_pending_sends(&mut self) {
        for result in self.sender.poll_results() {
            let success = result.error.is_none();

            match result.error {
                Some((code, detail)) => {
                    // A connection test has no recipient, so naming one would
                    // read as `to ''`.
                    let what = if result.recipient.is_empty() {
                        format!("Account {}", result.account_id)
                    } else {
                        format!("Send on account {}", result.account_id)
                    };
                    let recipient_detail = if result.recipient.is_empty() {
                        detail.clone()
                    } else {
                        format!("to '{}': {detail}", result.recipient)
                    };
                    self.report(result.account_id, (code, recipient_detail), &what);
                    callback::fire_on_email_error(
                        &self.amx_list,
                        result.account_id,
                        &result.recipient,
                        &detail,
                        &result.callback.name,
                        code.code(),
                    );
                }
                // A previous failure on this account should not keep being
                // reported by `email_errno` once something has gone through.
                None if !result.is_message => self.accounts.clear_error(result.account_id),
                None => {
                    self.accounts.clear_error(result.account_id);
                    callback::fire_on_email_sent(
                        &self.amx_list,
                        result.account_id,
                        &result.recipient,
                        &result.callback.name,
                    );
                }
            }

            // The outcome becomes the callback's first argument, so a gamemode
            // writes `public OnMailSent(success, playerid)`.
            let mut info = result.callback;
            if !info.name.is_empty() {
                info.prepend(CallbackParam::Int(i32::from(success)));
                callback::invoke_callback(&self.amx_list, &info);
            }
        }
    }
}

impl SampPlugin for EmailPlugin {
    fn on_load(&mut self) {}

    fn on_unload(&mut self) {
        if self.sender.pending_count() > 0 {
            Logger::info("Waiting for queued mail to leave before unloading...");
            let left = self.sender.drain(SHUTDOWN_GRACE);
            if left > 0 {
                // There is no on-disk queue: these mails are gone.
                Logger::warn(&format!(
                    "Unloading with {left} message(s) still queued — they will not be sent."
                ));
            }
        }
        Logger::info("Plugin unloaded.");
        Logger::flush();
    }

    fn on_amx_load(&mut self, amx: &Amx) {
        self.amx_list.push(Script::inspect(amx));
    }

    fn on_amx_unload(&mut self, amx: &Amx) {
        let ident = amx.ident();
        self.amx_list.retain(|script| script.ident != ident);

        // Reclaim the drafts the script owned, or a restart leaks every
        // handle. Accounts and templates survive on purpose: a /gmx should not
        // tear down a relay connection a filterscript is using.
        self.messages.destroy_by_amx(ident);
    }

    /// Fires on SA-MP (ProcessTick) and on open.mp native mode
    /// (ITimersComponent), which is what keeps sends off the main thread
    /// without a Pawn timer.
    fn on_tick(&mut self, _ctx: TickContext) {
        self.process_pending_sends();
    }

    fn on_omp_ready(&mut self) {
        Logger::info("Open Multiplayer native mode: all components ready.");
    }

    fn on_component_free(&mut self) {
        Logger::info("Open Multiplayer: a neighbouring component is being unloaded.");
    }
}
