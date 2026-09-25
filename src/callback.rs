//! Dispatching Pawn callbacks from the main thread.
//!
//! A send finishes on a worker, but the AMX is not thread-safe: the worker
//! produces a [`CallbackInfo`] and `on_tick` runs it. Nothing here is called
//! off the main thread.

use samp::amx::{Amx, AmxIdent, get as get_amx};
use samp::exec_public;
use samp::prelude::AmxExt;

use crate::logger::Logger;

/// A loaded script, and which of the plugin's broadcasts it listens to.
///
/// Looked up once when the script loads, not per message: the broadcasts go
/// to every script that defines them, and a server with a gamemode plus a
/// dozen filterscripts would otherwise pay a lookup in each of them for every
/// mail it sends.
#[derive(Debug, Clone, Copy)]
pub struct Script {
    pub ident: AmxIdent,
    on_sent: bool,
    on_error: bool,
}

impl Script {
    pub fn inspect(amx: &Amx) -> Self {
        Self {
            ident: amx.ident(),
            on_sent: amx.find_public("OnEmailSent").is_ok(),
            on_error: amx.find_public("OnEmailError").is_ok(),
        }
    }
}

/// A parameter waiting to be pushed onto the AMX stack.
#[derive(Debug, Clone)]
pub enum CallbackParam {
    Int(i32),
    Float(f32),
    String(String),
}

impl CallbackParam {
    /// The `format` letter that describes it.
    fn specifier(&self) -> char {
        match self {
            Self::Int(_) => 'd',
            Self::Float(_) => 'f',
            Self::String(_) => 's',
        }
    }
}

/// A Pawn public and the arguments to call it with. `format` is what the
/// gamemode passed to `email_send` — `d`/`i` int, `f` float, `s` string — so
/// it and `params` have the same length by construction.
#[derive(Debug, Clone)]
pub struct CallbackInfo {
    pub name: String,
    pub format: String,
    pub params: Vec<CallbackParam>,
}

impl CallbackInfo {
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            format: String::new(),
            params: Vec::new(),
        }
    }

    /// Prepends a parameter, which is how the send result becomes the
    /// callback's first argument.
    ///
    /// The specifier goes in with it: the push loop walks `format`, so a
    /// parameter without one is never pushed — and every later argument
    /// shifts by one, arriving as whatever was on the stack.
    ///
    /// Rebuilds the vector rather than calling `Vec::insert(0, ..)`, which
    /// would shift every element anyway.
    pub fn prepend(&mut self, param: CallbackParam) {
        self.format.insert(0, param.specifier());

        let mut params = Vec::with_capacity(self.params.len() + 1);
        params.push(param);
        params.append(&mut self.params);
        self.params = params;
    }
}

/// Invokes a Pawn callback, in the first loaded script that defines it.
pub fn invoke_callback(scripts: &[Script], info: &CallbackInfo) {
    if info.name.is_empty() {
        return;
    }

    for script in scripts {
        let Some(amx) = get_amx(script.ident) else {
            continue;
        };

        let Ok(idx) = amx.find_public(&info.name) else {
            continue;
        };

        // Parameters go on in reverse: the AMX stack grows downward, so the
        // last specifier must be pushed first for the callee to see them in
        // declaration order.
        let allocator = amx.allocator();
        let format_chars: Vec<char> = info.format.chars().collect();
        let mut push_ok = true;

        for (i, ch) in format_chars.iter().enumerate().rev() {
            let Some(param) = info.params.get(i) else {
                continue;
            };

            match (ch, param) {
                ('d' | 'i', CallbackParam::Int(v)) if amx.push(*v).is_err() => {
                    push_ok = false;
                    break;
                }
                ('f', CallbackParam::Float(v)) if amx.push(*v).is_err() => {
                    push_ok = false;
                    break;
                }
                ('s', CallbackParam::String(v)) => match allocator.allot_string(v) {
                    Ok(s) => {
                        if amx.push(s).is_err() {
                            push_ok = false;
                            break;
                        }
                    }
                    Err(_) => {
                        push_ok = false;
                        Logger::error("Failed to allocate an AMX string for a callback argument.");
                        break;
                    }
                },
                // A mismatch between the specifier and the stored parameter
                // cannot happen: both are built together in `email_send`.
                _ => {}
            }
        }

        if push_ok {
            let _ = amx.exec(idx);
        } else {
            Logger::error("A callback was aborted: failed to push parameters to the AMX stack.");
        }

        // Only the first script that defines the public runs it: a server
        // running a gamemode plus filterscripts that share a callback name
        // would otherwise handle one send result twice.
        break;
    }
}

/// Fires `OnEmailSent` in every loaded script, so a filterscript can keep the
/// record without the gamemode passing a callback to each send.
pub fn fire_on_email_sent(scripts: &[Script], account_id: i32, recipient: &str, callback: &str) {
    for script in scripts.iter().filter(|s| s.on_sent) {
        let Some(amx) = get_amx(script.ident) else {
            continue;
        };
        let _ = exec_public!(
            amx,
            "OnEmailSent",
            account_id,
            recipient => string,
            callback => string
        );
    }
}

/// Fires `OnEmailError` in every loaded script.
///
/// Unlike a send callback this is a broadcast: it is the plugin's diagnostic
/// channel, and a filterscript that wants to log failures should not have to
/// be the only script defining the public.
pub fn fire_on_email_error(
    scripts: &[Script],
    account_id: i32,
    recipient: &str,
    error_msg: &str,
    callback: &str,
    error_id: i32,
) {
    for script in scripts.iter().filter(|s| s.on_error) {
        let Some(amx) = get_amx(script.ident) else {
            continue;
        };

        let _ = exec_public!(
            amx,
            "OnEmailError",
            account_id,
            recipient => string,
            callback => string,
            error_msg => string,
            error_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepend_puts_the_result_first_and_keeps_the_rest_in_order() {
        let mut info = CallbackInfo {
            name: "OnMailSent".into(),
            format: "ds".into(),
            params: vec![
                CallbackParam::Int(7),
                CallbackParam::String("player".into()),
            ],
        };
        info.prepend(CallbackParam::Int(1));

        assert_eq!(info.params.len(), 3);
        assert!(matches!(info.params[0], CallbackParam::Int(1)));
        assert!(matches!(info.params[1], CallbackParam::Int(7)));
        assert!(matches!(info.params[2], CallbackParam::String(ref s) if s == "player"));
    }

    #[test]
    fn prepend_brings_the_specifier_with_it() {
        // The push loop walks `format`, so a parameter with no specifier is
        // never pushed and every later argument arrives shifted by one. This
        // is what made `OnMailSent(success, playerid)` see a stale playerid.
        let mut info = CallbackInfo {
            name: "OnMailSent".into(),
            format: "ds".into(),
            params: vec![
                CallbackParam::Int(7),
                CallbackParam::String("player".into()),
            ],
        };
        info.prepend(CallbackParam::Int(1));
        assert_eq!(info.format, "dds");
        assert_eq!(info.format.chars().count(), info.params.len());
    }

    #[test]
    fn prepend_onto_an_empty_parameter_list() {
        // A callback with no extras: `email_test(0, "OnTested")`. Without the
        // specifier nothing at all reached the public, not even the result.
        let mut info = CallbackInfo::empty();
        info.prepend(CallbackParam::Int(0));
        assert_eq!(info.params.len(), 1);
        assert_eq!(info.format, "d");
    }

    #[test]
    fn every_parameter_read_from_pawn_keeps_format_and_params_in_step() {
        let mut info = CallbackInfo {
            name: "OnMailSent".into(),
            format: "dfs".into(),
            params: vec![
                CallbackParam::Int(1),
                CallbackParam::Float(2.5),
                CallbackParam::String("x".into()),
            ],
        };
        info.prepend(CallbackParam::Float(0.0));
        assert_eq!(info.format, "fdfs");
        assert_eq!(info.format.chars().count(), info.params.len());
    }

    #[test]
    fn an_empty_callback_has_no_name() {
        assert!(CallbackInfo::empty().name.is_empty());
    }
}
