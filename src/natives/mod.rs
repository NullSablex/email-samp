pub mod account;
pub mod error;
pub mod message;
pub mod template;
pub mod util;

use samp::args::Args;
use samp::prelude::*;

use crate::callback::{CallbackInfo, CallbackParam};

/// Reads the `const callback[], const format[], {Float,_}:...` tail every
pub fn read_callback_args(args: &mut Args) -> CallbackInfo {
    let mut next_string = || {
        args.next_arg::<AmxString>()
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    let (name, format) = (next_string(), next_string());
    read_callback(args, &name, &format)
}

/// Reads the trailing `{Float,_}:...` arguments of a variadic native into a
fn read_callback(args: &mut Args, name: &str, format: &str) -> CallbackInfo {
    if name.is_empty() {
        return CallbackInfo::empty();
    }

    let mut info = CallbackInfo {
        name: name.to_string(),
        format: String::new(),
        params: Vec::new(),
    };

    for spec in format.chars() {
        let param = match spec {
            'd' | 'i' => args.next_arg::<Ref<i32>>().map(|v| CallbackParam::Int(*v)),
            'f' => args
                .next_arg::<Ref<f32>>()
                .map(|v| CallbackParam::Float(*v)),
            's' => args
                .next_arg::<AmxString>()
                .map(|v| CallbackParam::String(v.to_string())),
            // An unknown specifier consumes nothing: guessing a width would
            // desynchronise every argument after it.
            _ => None,
        };

        let Some(param) = param else {
            break;
        };

        info.format.push(spec);
        info.params.push(param);
    }

    info
}
