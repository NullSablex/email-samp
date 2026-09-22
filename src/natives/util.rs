use samp::native;
use samp::prelude::*;

use crate::address;
use crate::logger::Logger;
use crate::plugin::EmailPlugin;

impl EmailPlugin {
    /// `email_is_valid_address(const address[])`
    #[native(name = "email_is_valid_address")]
    pub fn email_is_valid_address(&mut self, _amx: &Amx, address: &AmxString) -> bool {
        address::is_valid_address(&address.to_string())
    }

    /// `email_force_reload_templates()` - drops the template cache.
    #[native(name = "email_force_reload_templates")]
    pub fn email_force_reload_templates(&mut self, _amx: &Amx) -> i32 {
        let dropped = self.templates.clear();
        Logger::info(&format!(
            "{dropped} template(s) dropped; the next send reads them from disk."
        ));
        i32::try_from(dropped).unwrap_or(i32::MAX)
    }

    /// `email_limit(limit)` - an EMAIL_LIMIT_* value, or 0 if unknown.
    #[native(name = "email_limit")]
    pub fn email_limit(&mut self, _amx: &Amx, limit: i32) -> i32 {
        limit_value(limit).map_or(0, |v| i32::try_from(v).unwrap_or(i32::MAX))
    }

    /// `email_log(level)` — 0 off, 1 error, 2 warn, 3 info, 4 everything.
    #[native(name = "email_log")]
    pub fn email_log(&mut self, _amx: &Amx, level: i32) -> bool {
        Logger::set_log_level(level);
        true
    }
}

/// The limits, in the order `EMAIL_LIMIT_*` declares them.
///
/// They live here, where they are enforced; the include only names them, so
/// the two cannot drift.
fn limit_value(limit: i32) -> Option<u64> {
    let value = match limit {
        0 => u64::try_from(crate::message::MAX_RECIPIENTS).ok()?,
        1 => u64::try_from(crate::message::MAX_HEADERS).ok()?,
        2 => crate::message::MAX_ATTACHMENT_BYTES,
        3 => u64::try_from(crate::message::MAX_DRAFTS).ok()?,
        4 => u64::try_from(crate::address::MAX_HEADER_LEN).ok()?,
        _ => return None,
    };
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::limit_value;

    #[test]
    fn every_limit_the_include_names_has_a_value_here_in_the_same_order() {
        let inc = include_str!("../../include/email_samp.inc.in");
        let start = inc.find("EMAIL_LIMIT_RECIPIENTS = 0").expect("limit enum");
        let names: Vec<&str> = inc[start..]
            .lines()
            .map(str::trim)
            .take_while(|line| !line.starts_with('}'))
            .filter(|line| line.starts_with("EMAIL_LIMIT_"))
            .collect();

        assert_eq!(names.len(), 5, "{names:?}");
        for (i, name) in names.iter().enumerate() {
            let value = limit_value(i32::try_from(i).expect("small")).expect(name);
            assert!(value > 0, "{name} is {value}");
        }
        assert_eq!(
            limit_value(i32::try_from(names.len()).expect("small")),
            None
        );
    }

    #[test]
    fn the_values_are_the_ones_the_plugin_enforces() {
        assert_eq!(limit_value(0), Some(100));
        assert_eq!(limit_value(2), Some(25 * 1024 * 1024));
        assert_eq!(limit_value(-1), None);
    }
}
