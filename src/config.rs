//! The `KEY=value` parser, with dotenv rules rather than `.ini` ones — so a
//! file written for `env_samp` parses identically here.
//!
//! - `#` starts a comment at the start of a line or after whitespace, never
//!   mid-token: `password=hunter#2` keeps the `#`. `;` also starts one.
//! - A leading `export ` is stripped.
//! - `'single quotes'` are literal; `"double quotes"` process `\n`, `\r`,
//!   `\t`, `\\` and `\"`. Quoting is the only way to keep the leading or
//!   trailing spaces a Gmail app password needs.
//! - Only the first `=` splits, so a password may contain one.
//! - Keys lose case and an optional `SMTP_` prefix, which is what lets one
//!   table serve the file and the process environment alike.
//! - A UTF-8 BOM is skipped.
//!
//! Nothing parsed here is ever logged.

use std::collections::HashMap;

/// Guards against being pointed at something that is not a config file.
pub const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Parses a file into normalised keys (lowercase, no `smtp_` prefix).
pub fn parse(contents: &str) -> HashMap<String, String> {
    strip_bom(contents).lines().filter_map(parse_line).collect()
}

/// Parses the one-line form `email_connect` takes: the same `KEY=value`
/// statements, separated by `;` or a line break.
///
/// A `;` inside quotes does not split, so `from_name="A; B"` survives. Same
/// grammar as the file on purpose: one parser means one set of rules for a
/// password, wherever it was written.
pub fn parse_inline(settings: &str) -> HashMap<String, String> {
    split_statements(settings)
        .into_iter()
        .filter_map(parse_line)
        .collect()
}

fn split_statements(input: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0;

    for (i, ch) in input.char_indices() {
        // Inside double quotes a backslash escapes the next character, so
        // `"say \"hi\""` does not end the quote early. Single quotes are
        // literal, as in the file grammar.
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, ch) {
            (Some('"'), '\\') => escaped = true,
            (None, '\'' | '"') => quote = Some(ch),
            (Some(q), c) if c == q => quote = None,
            (None, ';' | '\n') => {
                out.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&input[start..]);
    out
}

/// Lowercases and drops an optional `smtp_` prefix.
pub fn normalize_key(key: &str) -> String {
    let key = key.trim().to_ascii_lowercase();
    key.strip_prefix("smtp_").unwrap_or(&key).to_string()
}

fn parse_line(raw: &str) -> Option<(String, String)> {
    let line = raw.strip_suffix('\r').unwrap_or(raw).trim();

    if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
        return None;
    }

    // `export FOO=bar` is parsed rather than skipped: a file that is also
    // sourced by a shell is a normal thing to have, and dropping the line
    // would silently lose the setting.
    let line = line.strip_prefix("export ").unwrap_or(line).trim();

    let (key, rest) = line.split_once('=')?;
    let key = normalize_key(key);
    if key.is_empty() {
        return None;
    }

    Some((key, parse_value(rest.trim())))
}

fn parse_value(rest: &str) -> String {
    if let Some(inner) = strip_quoted(rest, '\'') {
        return inner.to_string();
    }
    if let Some(inner) = strip_quoted(rest, '"') {
        return unescape(inner);
    }
    strip_inline_comment(rest).trim_end().to_string()
}

fn strip_quoted(s: &str, quote: char) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && s.starts_with(quote) && s.ends_with(quote) {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

/// A `#` only ends the value when it follows whitespace. Without that rule a
/// password containing `#` would be silently truncated — which is exactly the
/// kind of failure that looks like "the server rejects my password".
fn strip_inline_comment(rest: &str) -> &str {
    let mut prev_ws = true;
    for (i, ch) in rest.char_indices() {
        if ch == '#' && prev_ws {
            return &rest[..i];
        }
        prev_ws = ch.is_whitespace();
    }
    rest
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            // An unknown escape keeps both characters rather than eating the
            // backslash: a Windows path in a value stays intact.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }

    out
}

fn strip_bom(input: &str) -> &str {
    input.strip_prefix('\u{FEFF}').unwrap_or(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get<'a>(map: &'a HashMap<String, String>, key: &str) -> &'a str {
        map.get(key).map(String::as_str).unwrap_or("<missing>")
    }

    #[test]
    fn parses_plain_pairs() {
        let v = parse("host = smtp.example.com\nuser = bot@example.com\n");
        assert_eq!(get(&v, "host"), "smtp.example.com");
        assert_eq!(get(&v, "user"), "bot@example.com");
    }

    #[test]
    fn the_smtp_prefix_and_case_are_dropped() {
        // The same table serves SMTP_HOST from the environment and host= from
        // an ini file.
        let v = parse("SMTP_HOST=a\nSmTp_User=b\nPASSWORD=c\n");
        assert_eq!(get(&v, "host"), "a");
        assert_eq!(get(&v, "user"), "b");
        assert_eq!(get(&v, "password"), "c");
    }

    #[test]
    fn export_lines_are_parsed_not_skipped() {
        let v = parse("export SMTP_HOST=a\n");
        assert_eq!(get(&v, "host"), "a");
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let v = parse("# top\n\n; ini style\nhost=h   # trailing\n");
        assert_eq!(get(&v, "host"), "h");
    }

    #[test]
    fn a_hash_inside_a_value_is_kept() {
        // The bug this rule exists to prevent: a truncated password that
        // looks like "the relay rejects my credentials".
        let v = parse("password=hunter#2\n");
        assert_eq!(get(&v, "password"), "hunter#2");
    }

    #[test]
    fn quotes_preserve_spaces_for_app_passwords() {
        let v = parse("password=\"abcd efgh ijkl mnop\"\n");
        assert_eq!(get(&v, "password"), "abcd efgh ijkl mnop");
    }

    #[test]
    fn single_quotes_are_literal() {
        let v = parse("password='a\\nb'\n");
        assert_eq!(get(&v, "password"), "a\\nb");
    }

    #[test]
    fn double_quotes_process_escapes() {
        let v = parse("a=\"line1\\nline2\"\nb=\"say \\\"hi\\\"\"\n");
        assert_eq!(get(&v, "a"), "line1\nline2");
        assert_eq!(get(&v, "b"), "say \"hi\"");
    }

    #[test]
    fn an_unknown_escape_keeps_the_backslash() {
        let v = parse("path=\"C:\\Users\\bot\"\n");
        assert_eq!(get(&v, "path"), "C:\\Users\\bot");
    }

    #[test]
    fn only_the_first_equals_splits() {
        let v = parse("password=a=b=c\n");
        assert_eq!(get(&v, "password"), "a=b=c");
    }

    #[test]
    fn an_empty_value_is_kept() {
        let v = parse("password=\n");
        assert_eq!(get(&v, "password"), "");
    }

    #[test]
    fn a_bom_does_not_break_the_first_key() {
        let v = parse("\u{FEFF}host=h\n");
        assert_eq!(get(&v, "host"), "h");
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let v = parse("host=h\r\nuser=u\r\n");
        assert_eq!(get(&v, "host"), "h");
        assert_eq!(get(&v, "user"), "u");
    }

    #[test]
    fn lines_without_an_equals_are_skipped() {
        let v = parse("garbage\nhost=h\n");
        assert_eq!(v.len(), 1);
        assert_eq!(get(&v, "host"), "h");
    }

    #[test]
    fn the_inline_form_splits_on_semicolons_outside_quotes() {
        let map = parse_inline("port=465; from_name=\"A; B\"\nSMTP_TIMEOUT=10;");
        assert_eq!(get(&map, "port"), "465");
        assert_eq!(get(&map, "from_name"), "A; B");
        assert_eq!(get(&map, "timeout"), "10");
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn an_escaped_quote_does_not_end_an_inline_value() {
        let map = parse_inline(r#"from_name="The \"Best\"; RP"; port=465"#);
        assert_eq!(get(&map, "from_name"), r#"The "Best"; RP"#);
        assert_eq!(get(&map, "port"), "465");
    }

    #[test]
    fn an_empty_inline_string_is_an_empty_table() {
        assert!(parse_inline("").is_empty());
        assert!(parse_inline(" ; ;").is_empty());
    }

    #[test]
    fn a_later_key_wins() {
        let v = parse("host=first\nhost=second\n");
        assert_eq!(get(&v, "host"), "second");
    }
}
