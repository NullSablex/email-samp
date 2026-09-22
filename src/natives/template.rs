use std::path::Path;

use samp::args::Args;
use samp::native;
use samp::prelude::*;

use crate::error::EmailError;
use crate::format::{self, Piece, Value};
use crate::message::Var;
use crate::plugin::EmailPlugin;

impl EmailPlugin {
    /// `email_set_template(message, const path[])`
    #[native(name = "email_set_template")]
    pub fn email_set_template(&mut self, _amx: &Amx, message_id: i32, path: &AmxString) -> bool {
        if self.messages.get(message_id).is_none() {
            return self.invalid_message(message_id);
        }
        let template = self.templates.get(Path::new(&path.to_string()));
        self.edit(message_id, |d| {
            d.template = Some(template?);
            Ok(())
        })
    }

    /// `email_set_var(message, const key[], const value[], {Float,_}:...)`
    #[native(name = "email_set_var", raw)]
    pub fn email_set_var(&mut self, _amx: &Amx, mut args: Args) -> bool {
        let message_id = args.next_arg::<i32>().unwrap_or(0);
        let (Some(key), Some(value)) = (args.next_arg::<AmxString>(), args.next_arg::<AmxString>())
        else {
            return self.invalid_message(message_id);
        };

        let (key, value) = (key.to_string(), value.to_string());
        // With nothing after it the value is text as written, so a nickname
        // containing '%' needs no escaping.
        let (text, raw) = if args.count() > 3 {
            read_formatted(&value, &mut args)
        } else {
            (value, false)
        };

        self.store_var(message_id, key, text, raw)
    }

    fn store_var(&mut self, message_id: i32, key: String, text: String, raw: bool) -> bool {
        self.edit(message_id, |d| {
            if key.is_empty() {
                return Err(EmailError::BuildFailed.because("a template variable needs a name"));
            }
            d.vars.insert(key, Var { text, raw });
            Ok(())
        })
    }
}

/// Reads one argument per specifier, as the type that specifier names — the
/// only type information a variadic call carries.
fn read_formatted(value: &str, args: &mut Args) -> (String, bool) {
    let pieces = format::parse(value);
    let mut values = Vec::with_capacity(format::holes(&pieces));

    for piece in &pieces {
        let read = match piece {
            Piece::Text(_) => continue,
            Piece::Int => args.next_arg::<Ref<i32>>().map(|v| Value::Int(*v)),
            Piece::Float(_) => args.next_arg::<Ref<f32>>().map(|v| Value::Float(*v)),
            Piece::Str | Piece::Raw => args
                .next_arg::<AmxString>()
                .map(|v| Value::Str(v.to_string())),
        };
        let Some(value) = read else {
            break;
        };
        values.push(value);
    }

    (format::render(&pieces, &values), format::is_raw(&pieces))
}
