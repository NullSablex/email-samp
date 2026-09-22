//! Turning a configuration source into a ready-to-connect [`Settings`].
//!
//! One table of keys, filled from whichever source the gamemode used — the
//! process environment, an SMTP URL, a file, or the settings string
//! `email_connect` takes. This is the only place a value is parsed or
//! validated, so a key means the same thing wherever it was written.
//!
//! `.env` is not among the sources: [env_samp] reads it, and two parsers for
//! one format would eventually disagree about a password with a `#` in it.
//! The glue is the `email_setup_env` stock in the include.
//!
//! [env_samp]: https://github.com/NullSablex/env-samp

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use crate::error::{EmailError, Fail};
use crate::options::{Charset, EmailOptions, Encryption};

/// Every recognised key, after [`crate::config::normalize_key`] and
/// [`canonical`]. Listed so a typo is reported: `SMTP_PASSWD` would otherwise
/// look exactly like "no password configured".
const KNOWN_KEYS: &[&str] = &[
    "url",
    "host",
    "user",
    "password",
    "port",
    "encryption",
    "from",
    "from_name",
    "helo",
    "timeout",
    "pool_size",
    "tls_ca",
    "tls_verify",
    "retries",
    "rate_limit",
    "queue_limit",
    "dry_run",
    "allow_plaintext_auth",
    "charset",
];

/// Folds the accepted spellings of a key onto the one [`KNOWN_KEYS`] lists.
fn canonical(key: &str) -> &str {
    match key {
        "username" => "user",
        "pass" => "password",
        // `tls` reads better than `encryption` in a URL.
        "tls" => "encryption",
        other => other,
    }
}

/// Where a configuration came from. Logged on success so an operator can see
/// which source actually won.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Environment,
    Url,
    File(String),
    Code,
}

impl Source {
    pub fn describe(&self) -> String {
        match self {
            Self::Environment => String::from("the process environment"),
            Self::Url => String::from("the supplied URL"),
            Self::File(path) => format!("'{path}'"),
            Self::Code => String::from("email_connect"),
        }
    }
}

pub struct Settings {
    pub host: String,
    pub user: String,
    pub password: String,
    pub options: EmailOptions,
}

/// Resolves whatever `source` names.
///
/// - empty: try the process environment, then `smtp.ini`
/// - `smtp://` or `smtps://`: a URL
/// - anything else: a file path
pub fn resolve(source: &str) -> Result<(Settings, Source), Fail> {
    let source = source.trim();

    if source.is_empty() {
        return discover();
    }

    if is_url(source) {
        return from_url(source).map(|s| (s, Source::Url));
    }

    from_file(Path::new(source)).map(|s| (s, Source::File(source.to_string())))
}

/// `email_connect(host, user, password, settings)`: the credentials come as
/// separate arguments — so a password needs no quoting or escaping — and
/// everything else as the inline `key=value; key=value` form.
pub fn from_connect(
    host: &str,
    user: &str,
    password: &str,
    inline: &str,
) -> Result<Settings, Fail> {
    let mut values = crate::config::parse_inline(inline);
    for (key, value) in [("host", host), ("user", user), ("password", password)] {
        values.insert(key.to_string(), value.to_string());
    }
    from_map(&values)
}

fn is_url(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.starts_with("smtp://") || lower.starts_with("smtps://")
}

/// Auto-discovery, in order of how explicitly the operator chose it.
///
/// The environment wins over the file: a container that sets `SMTP_URL` should
/// not be overridden by an `smtp.ini` that happened to be baked into the
/// image.
fn discover() -> Result<(Settings, Source), Fail> {
    let env = read_environment();
    if !env.is_empty() {
        return from_map(&env).map(|s| (s, Source::Environment));
    }

    let path = Path::new("smtp.ini");
    if path.is_file() {
        return from_file(path).map(|s| (s, Source::File(String::from("smtp.ini"))));
    }

    Err(EmailError::ConfigFailed.because(
        "no configuration found: set SMTP_URL (or SMTP_HOST / SMTP_USER / SMTP_PASSWORD) \
         in the environment, create smtp.ini, or pass a URL to email_setup",
    ))
}

/// Collects `SMTP_*` process environment variables into the normalised table.
///
/// Only the prefixed names are read. Picking up a bare `HOST` or `USER` would
/// mean the plugin's behaviour changed with whatever the shell happened to
/// export, which is exactly the sort of thing nobody debugs successfully.
fn read_environment() -> HashMap<String, String> {
    std::env::vars()
        .filter(|(_, value)| !value.is_empty())
        .filter_map(|(key, value)| {
            let lower = key.to_ascii_lowercase();
            lower
                .strip_prefix("smtp_")
                .map(|name| (name.to_string(), value))
        })
        .collect()
}

fn from_file(path: &Path) -> Result<Settings, Fail> {
    let unreadable = |e: std::io::Error| {
        EmailError::ConfigFailed.because(format!("could not read '{}': {e}", path.display()))
    };

    let meta = std::fs::metadata(path).map_err(unreadable)?;
    if !meta.is_file() {
        return Err(
            EmailError::ConfigFailed.because(format!("'{}' is not a regular file", path.display()))
        );
    }
    if meta.len() > crate::config::MAX_CONFIG_BYTES {
        return Err(EmailError::ConfigFailed.because(format!(
            "'{}' is larger than {} bytes - is that really the config file?",
            path.display(),
            crate::config::MAX_CONFIG_BYTES
        )));
    }

    let contents = std::fs::read_to_string(path).map_err(unreadable)?;
    from_map(&crate::config::parse(&contents))
}

/// `smtps://user:pass@host:465` or `smtp://user:pass@host:587?tls=none`.
///
/// Parsed with the `url` crate rather than by hand because the userinfo is
/// percent-encoded: a password containing `@`, `/` or `:` is only
/// unambiguous once it is decoded, and getting that wrong would silently
/// authenticate with the wrong string.
pub fn from_url(raw: &str) -> Result<Settings, Fail> {
    from_map(&url_table(raw)?)
}

fn url_table(raw: &str) -> Result<HashMap<String, String>, Fail> {
    let parsed = url::Url::parse(raw).map_err(|e| {
        // The URL carries the password, so it is never echoed.
        EmailError::ConfigFailed.because(format!("the SMTP URL could not be parsed: {e}"))
    })?;

    let Some(host) = parsed.host_str() else {
        return Err(EmailError::ConfigFailed.because("the SMTP URL has no host"));
    };

    // `smtps` is implicit TLS on 465; `smtp` is STARTTLS on 587. Both are
    // overridable by an explicit port or a `?tls=` parameter below.
    let scheme_tls = if parsed.scheme().eq_ignore_ascii_case("smtps") {
        "tls"
    } else {
        "starttls"
    };

    let mut values = HashMap::from([
        (String::from("host"), host.to_string()),
        (String::from("encryption"), scheme_tls.to_string()),
    ]);

    let user = percent_decode(parsed.username());
    if !user.is_empty() {
        values.insert(String::from("user"), user);
    }
    if let Some(password) = parsed.password() {
        values.insert(String::from("password"), percent_decode(password));
    }
    if let Some(port) = parsed.port() {
        values.insert(String::from("port"), port.to_string());
    }

    // Query parameters cover the rest, so a single string can carry a whole
    // configuration: smtp://u:p@h?tls=none&from=noreply@example.com
    for (key, value) in parsed.query_pairs() {
        let key = crate::config::normalize_key(&key);
        values.insert(canonical(&key).to_string(), value.to_string());
    }

    Ok(values)
}

fn percent_decode(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}

/// Maps a normalised table onto [`Settings`].
pub fn from_map(raw: &HashMap<String, String>) -> Result<Settings, Fail> {
    let mut values: HashMap<String, String> = HashMap::new();
    for (key, value) in raw {
        // Two spellings of one key (`user` and `username`) would otherwise
        // resolve in whatever order the table iterates: a coin flip.
        if values
            .insert(canonical(key).to_string(), value.clone())
            .is_some()
        {
            return Err(EmailError::ConfigFailed.because(format!(
                "'{}' is set twice under different names",
                canonical(key)
            )));
        }
    }

    // A misspelled key is reported rather than ignored. "SMTP_PASSWD is not a
    // known key" is a fixable message; silently connecting with no password
    // and failing on 535 is not.
    if let Some(unknown) = values.keys().find(|k| !KNOWN_KEYS.contains(&k.as_str())) {
        return Err(EmailError::ConfigFailed.because(format!(
            "'{unknown}' is not a recognised setting (expected one of: {})",
            KNOWN_KEYS.join(", ")
        )));
    }

    // A url= key is expanded first and the other keys override it. That way
    // SMTP_URL plus SMTP_FROM works.
    if let Some(url) = values.remove("url") {
        let mut expanded = url_table(&url)?;
        expanded.extend(values);
        values = expanded;
    }

    let take = |key: &str| values.get(key).cloned().unwrap_or_default();

    let host = take("host").trim().to_string();
    if host.is_empty() {
        return Err(EmailError::ConfigFailed
            .because("no relay host configured (set SMTP_HOST, or host= in the file)"));
    }

    let mut opts = EmailOptions::default();
    let get = |key: &str| values.get(key).map(String::as_str);

    if let Some(raw) = get("encryption") {
        opts.encryption = parse_encryption(raw)?;
    }
    if let Some(port) = positive::<u16>(get("port"), "port", "a number between 1 and 65535")? {
        opts.port = Some(port);
    }
    if let Some(from) = get("from") {
        if !crate::address::is_valid_address(from) {
            return Err(bad_value("from", "a valid email address"));
        }
        opts.from_address = from.trim().to_string();
    }
    if let Some(name) = header_value(get("from_name"), "from_name")? {
        opts.from_name = name;
    }
    if let Some(helo) = header_value(get("helo"), "helo")? {
        opts.helo_name = helo;
    }
    if let Some(secs) = positive(get("timeout"), "timeout", "a number of seconds")? {
        opts.timeout_secs = secs;
    }
    if let Some(size) = positive(get("pool_size"), "pool_size", "a positive number")? {
        opts.pool_size = size;
    }
    if let Some(limit) = positive(get("queue_limit"), "queue_limit", "a positive number")? {
        opts.queue_limit = limit;
    }
    if let Some(retries) = number(get("retries"), "retries", "a number (0 disables)")? {
        opts.retries = retries;
    }
    if let Some(rate) = number(
        get("rate_limit"),
        "rate_limit",
        "messages per minute (0 = unlimited)",
    )? {
        opts.rate_limit = rate;
    }
    if let Some(ca) = get("tls_ca").filter(|ca| !ca.is_empty()) {
        opts.tls_ca = Some(ca.to_string());
    }
    if let Some(raw) = get("charset") {
        opts.charset = parse_charset(raw)?;
    }
    if let Some(raw) = get("allow_plaintext_auth") {
        opts.allow_plaintext_auth = parse_bool(raw).ok_or_else(|| {
            bad_value("allow_plaintext_auth", "1/0, true/false, yes/no or on/off")
        })?;
    }
    if let Some(raw) = get("dry_run") {
        opts.dry_run = parse_bool(raw)
            .ok_or_else(|| bad_value("dry_run", "1/0, true/false, yes/no or on/off"))?;
    }
    if let Some(raw) = get("tls_verify") {
        opts.tls_verify_cert = parse_bool(raw)
            .ok_or_else(|| bad_value("tls_verify", "1/0, true/false, yes/no or on/off"))?;
    }

    Ok(Settings {
        host,
        user: take("user"),
        password: take("password"),
        options: opts,
    })
}

/// A number, zero allowed.
fn number<T: FromStr>(raw: Option<&str>, key: &str, expected: &str) -> Result<Option<T>, Fail> {
    raw.map(|raw| raw.trim().parse().map_err(|_| bad_value(key, expected)))
        .transpose()
}

/// A number where zero is meaningless (a port, a timeout, a pool size).
fn positive<T: FromStr + Default + PartialEq>(
    raw: Option<&str>,
    key: &str,
    expected: &str,
) -> Result<Option<T>, Fail> {
    match number::<T>(raw, key, expected)? {
        Some(value) if value == T::default() => Err(bad_value(key, expected)),
        other => Ok(other),
    }
}

/// A value that ends up in a header, and so may not carry a line break.
fn header_value(raw: Option<&str>, key: &str) -> Result<Option<String>, Fail> {
    raw.map(|value| {
        crate::address::check_header_value(value)
            .map(|()| value.to_string())
            .map_err(|(_, detail)| EmailError::ConfigFailed.because(format!("{key}: {detail}")))
    })
    .transpose()
}

fn parse_encryption(raw: &str) -> Result<Encryption, Fail> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" | "plain" | "plaintext" | "off" | "0" => Ok(Encryption::None),
        "starttls" | "start_tls" | "submission" => Ok(Encryption::StartTls),
        "tls" | "ssl" | "smtps" | "implicit" => Ok(Encryption::Tls),
        _ => Err(bad_value("encryption", "none, starttls or tls")),
    }
}

fn parse_charset(raw: &str) -> Result<Charset, Fail> {
    match raw.trim().to_ascii_lowercase().replace(['-', '_'], "") {
        v if v == "windows1252" || v == "cp1252" || v == "latin1" || v == "ansi" => {
            Ok(Charset::Windows1252)
        }
        v if v == "windows1251" || v == "cp1251" || v == "cyrillic" => Ok(Charset::Windows1251),
        v if v == "utf8" => Ok(Charset::Utf8),
        _ => Err(bad_value("charset", "windows-1252, windows-1251 or utf-8")),
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn bad_value(key: &str, expected: &str) -> Fail {
    // Names the key and what was expected, never the value: these tables hold
    // a password.
    EmailError::ConfigFailed.because(format!("'{key}' must be {expected}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn a_minimal_table_yields_the_defaults() {
        let s = from_map(&map(&[
            ("host", "smtp.example.com"),
            ("user", "bot@example.com"),
        ]))
        .expect("valid");
        assert_eq!(s.host, "smtp.example.com");
        assert_eq!(s.user, "bot@example.com");
        assert_eq!(s.options.encryption, Encryption::StartTls);
        assert_eq!(s.options.effective_port(), 587);
    }

    #[test]
    fn a_missing_host_is_reported() {
        let Err((code, message)) = from_map(&map(&[("user", "u")])) else {
            panic!("expected a missing host to fail");
        };
        assert_eq!(code, EmailError::ConfigFailed);
        assert!(message.contains("SMTP_HOST"));
    }

    #[test]
    fn a_misspelled_key_is_reported_not_ignored() {
        let Err((_, message)) = from_map(&map(&[("host", "h"), ("passwd", "x")])) else {
            panic!("expected an unknown key to fail");
        };
        assert!(message.contains("passwd"));
    }

    #[test]
    fn aliases_are_accepted() {
        let s = from_map(&map(&[("host", "h"), ("username", "u"), ("pass", "p")])).expect("valid");
        assert_eq!(s.user, "u");
        assert_eq!(s.password, "p");
    }

    #[test]
    fn two_spellings_of_one_key_are_refused() {
        let Err((_, message)) = from_map(&map(&[("host", "h"), ("user", "a"), ("username", "b")]))
        else {
            panic!("expected an ambiguous key to fail");
        };
        assert!(message.contains("'user'"));
    }

    #[test]
    fn encryption_spellings_all_resolve() {
        for (raw, expected) in [
            ("none", Encryption::None),
            ("PLAIN", Encryption::None),
            ("starttls", Encryption::StartTls),
            ("submission", Encryption::StartTls),
            ("tls", Encryption::Tls),
            ("SSL", Encryption::Tls),
            ("smtps", Encryption::Tls),
        ] {
            let s = from_map(&map(&[("host", "h"), ("encryption", raw)])).expect("valid");
            assert_eq!(s.options.encryption, expected, "for {raw}");
        }
    }

    #[test]
    fn a_bad_value_names_the_key_but_never_the_value() {
        let Err((_, message)) = from_map(&map(&[("host", "h"), ("port", "not-a-number")])) else {
            panic!("expected a bad port to fail");
        };
        assert!(message.contains("port"));
        assert!(!message.contains("not-a-number"));
    }

    #[test]
    fn the_charset_spellings_all_resolve() {
        for (raw, expected) in [
            ("windows-1252", Charset::Windows1252),
            ("cp1252", Charset::Windows1252),
            ("latin1", Charset::Windows1252),
            ("windows-1251", Charset::Windows1251),
            ("UTF-8", Charset::Utf8),
            ("utf8", Charset::Utf8),
        ] {
            let s = from_map(&map(&[("host", "h"), ("charset", raw)])).expect("valid");
            assert_eq!(s.options.charset, expected, "for {raw}");
        }
        assert!(from_map(&map(&[("host", "h"), ("charset", "klingon")])).is_err());
    }

    #[test]
    fn dry_run_is_off_unless_asked_for() {
        let s = from_map(&map(&[("host", "h")])).expect("valid");
        assert!(!s.options.dry_run);
        let s = from_map(&map(&[("host", "h"), ("dry_run", "yes")])).expect("valid");
        assert!(s.options.dry_run);
        assert!(from_map(&map(&[("host", "h"), ("dry_run", "maybe")])).is_err());
    }

    #[test]
    fn booleans_accept_the_usual_spellings() {
        for raw in ["0", "false", "NO", "off"] {
            let s = from_map(&map(&[("host", "h"), ("tls_verify", raw)])).expect("valid");
            assert!(!s.options.tls_verify_cert, "for {raw}");
        }
        for raw in ["1", "true", "YES", "on"] {
            let s = from_map(&map(&[("host", "h"), ("tls_verify", raw)])).expect("valid");
            assert!(s.options.tls_verify_cert, "for {raw}");
        }
    }

    // --- URLs ---------------------------------------------------------------

    #[test]
    fn smtps_means_implicit_tls_on_465() {
        let s = from_url("smtps://bot%40example.com:secret@smtp.example.com").expect("valid");
        assert_eq!(s.host, "smtp.example.com");
        assert_eq!(s.user, "bot@example.com");
        assert_eq!(s.password, "secret");
        assert_eq!(s.options.encryption, Encryption::Tls);
        assert_eq!(s.options.effective_port(), 465);
    }

    #[test]
    fn smtp_means_starttls_on_587() {
        let s = from_url("smtp://user:pw@smtp.example.com").expect("valid");
        assert_eq!(s.options.encryption, Encryption::StartTls);
        assert_eq!(s.options.effective_port(), 587);
    }

    #[test]
    fn an_explicit_port_wins_over_the_scheme() {
        let s = from_url("smtp://user:pw@smtp.example.com:2525").expect("valid");
        assert_eq!(s.options.effective_port(), 2525);
    }

    #[test]
    fn a_percent_encoded_password_is_decoded() {
        // The reason the url crate is used instead of splitting on '@': this
        // password contains the delimiter.
        let s = from_url("smtp://user:p%40ss%2Fword@smtp.example.com").expect("valid");
        assert_eq!(s.password, "p@ss/word");
    }

    #[test]
    fn query_parameters_carry_the_rest_of_the_configuration() {
        let s =
            from_url("smtp://u:p@smtp.example.com/?tls=none&from=noreply@example.com&pool_size=8")
                .expect("valid");
        assert_eq!(s.options.encryption, Encryption::None);
        assert_eq!(s.options.from_address, "noreply@example.com");
        assert_eq!(s.options.pool_size, 8);
    }

    #[test]
    fn a_malformed_url_never_echoes_the_password() {
        let Err((_, message)) = from_url("smtp://user:hunter2@") else {
            panic!("expected a malformed URL to fail");
        };
        assert!(!message.contains("hunter2"));
    }

    #[test]
    fn a_url_key_in_a_table_is_expanded_and_siblings_override_it() {
        let s = from_map(&map(&[
            ("url", "smtps://u:p@smtp.example.com"),
            ("from_name", "Los Santos RP"),
            ("pool_size", "2"),
        ]))
        .expect("valid");
        assert_eq!(s.options.encryption, Encryption::Tls);
        assert_eq!(s.options.from_name, "Los Santos RP");
        assert_eq!(s.options.pool_size, 2);
    }

    #[test]
    fn a_hostile_from_name_is_refused_in_configuration_too() {
        let Err((code, _)) = from_map(&map(&[
            ("host", "h"),
            ("from_name", "Server\r\nBcc: evil@evil.tld"),
        ])) else {
            panic!("expected an injected from_name to be refused");
        };
        assert_eq!(code, EmailError::ConfigFailed);
    }

    // --- Source selection ---------------------------------------------------

    #[test]
    fn a_url_source_is_detected_by_scheme() {
        assert!(is_url("smtp://h"));
        assert!(is_url("SMTPS://h"));
        assert!(!is_url("smtp.ini"));
        assert!(!is_url("/etc/mail/smtp.ini"));
    }

    #[test]
    fn a_missing_file_is_reported_with_its_path() {
        let Err((code, message)) = resolve("/definitely/not/here.ini") else {
            panic!("expected a missing file to fail");
        };
        assert_eq!(code, EmailError::ConfigFailed);
        assert!(message.contains("here.ini"));
    }

    // --- email_connect ------------------------------------------------------

    #[test]
    fn connect_takes_credentials_apart_and_the_rest_inline() {
        let s = from_connect(
            "smtp.example.com",
            "bot@example.com",
            "p;a=ss\"word",
            "port=2525; retries=0; rate_limit=30; from_name=\"Los Santos; RP\"",
        )
        .expect("valid");
        assert_eq!(s.password, "p;a=ss\"word");
        assert_eq!(s.options.effective_port(), 2525);
        assert_eq!(s.options.retries, 0);
        assert_eq!(s.options.rate_limit, 30);
        assert_eq!(s.options.from_name, "Los Santos; RP");
    }

    #[test]
    fn the_string_email_setup_env_builds_is_read_back_intact() {
        // The exact shape the stock in email_samp.inc.in writes: every value
        // quoted, `"` and `\` escaped, each setting closed by `;`.
        let s = from_connect(
            "h",
            "u@e.com",
            "p",
            r#"port="2525";from_name="The \"Best\"; RP";tls_ca="C:\\certs\\ca.pem";"#,
        )
        .expect("valid");
        assert_eq!(s.options.effective_port(), 2525);
        assert_eq!(s.options.from_name, r#"The "Best"; RP"#);
        assert_eq!(s.options.tls_ca.as_deref(), Some(r"C:\certs\ca.pem"));
    }

    #[test]
    fn connect_with_no_settings_uses_the_defaults() {
        let s = from_connect("h", "u@e.com", "p", "").expect("valid");
        assert_eq!(s.options.retries, 2);
        assert_eq!(s.options.queue_limit, 1000);
        assert_eq!(s.options.rate_limit, 0);
    }

    #[test]
    fn connect_settings_are_checked_like_any_other_source() {
        let Err((code, message)) = from_connect("h", "u", "p", "queue_limit=0") else {
            panic!("expected a zero queue limit to fail");
        };
        assert_eq!(code, EmailError::ConfigFailed);
        assert!(message.contains("queue_limit"));
        assert!(from_connect("h", "u", "p", "passwd=x").is_err());
    }

    #[test]
    fn source_descriptions_are_human_readable() {
        assert_eq!(Source::Environment.describe(), "the process environment");
        assert_eq!(
            Source::File(String::from("smtp.ini")).describe(),
            "'smtp.ini'"
        );
    }
}
