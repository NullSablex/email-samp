//! Address parsing and header-injection defence.
//!
//! A header ends at a bare CR or LF, so a nickname carrying
//! `\r\nBcc: attacker@evil.tld` does not make a strange subject — it makes a
//! new header, and every password-reset mail gets blind-copied to the
//! attacker. Two rules, applied in the natives before a value is stored:
//!
//! 1. Anything that becomes a header is **rejected**, not stripped, when it
//!    holds CR or LF, so the gamemode learns its input is hostile.
//! 2. Addresses are **parsed** by `lettre::Address` (RFC 5321), never matched
//!    with a pattern.
//!
//! The body is exempt: CRLF is legal there and lettre encodes it, so it can
//! never reach the header block.

use std::str::FromStr;

use lettre::message::Mailbox;

use crate::error::EmailError;

/// Longest value accepted for one header field.
///
/// RFC 5322 sets no ceiling but relays do (commonly 998 octets per line), and
/// an unbounded value from Pawn would just drop the session.
pub const MAX_HEADER_LEN: usize = 900;

/// Rejects a value that cannot safely become a header.
///
/// NUL counts: it ends the C string on the way out, truncating the header.
pub fn check_header_value(value: &str) -> Result<(), (EmailError, String)> {
    if value.contains(['\r', '\n', '\0']) {
        return Err((
            EmailError::HeaderInjection,
            String::from(
                "the value contains a line break or NUL, which would inject or truncate a header",
            ),
        ));
    }

    if value.len() > MAX_HEADER_LEN {
        return Err((
            EmailError::HeaderInjection,
            format!("the value is longer than the {MAX_HEADER_LEN} bytes allowed in a header"),
        ));
    }

    Ok(())
}

/// Validates a custom header *name*.
///
/// RFC 5322 calls this a field name: printable US-ASCII except colon. Anything
/// else — a space, a colon, a non-ASCII byte — would either be reinterpreted
/// as part of the previous header or rejected by the relay.
pub fn check_header_name(name: &str) -> Result<(), (EmailError, String)> {
    if name.is_empty() {
        return Err((
            EmailError::HeaderInjection,
            String::from("a header name cannot be empty"),
        ));
    }

    let valid = name.bytes().all(|b| b.is_ascii_graphic() && b != b':');

    if !valid {
        return Err((
            EmailError::HeaderInjection,
            String::from(
                "a header name may only contain printable ASCII characters other than ':'",
            ),
        ));
    }

    check_header_value(name)
}

/// Parses `address` (and an optional display name) into a mailbox.
///
/// An empty `name` yields a bare `<address>` rather than `"" <address>`, which
/// some relays log oddly and which adds nothing.
pub fn parse_mailbox(address: &str, name: &str) -> Result<Mailbox, (EmailError, String)> {
    let address = address.trim();

    // Checked before parsing so the caller gets the injection code rather than
    // a vaguer parse error for the same input.
    check_header_value(address)?;

    let parsed = lettre::Address::from_str(address).map_err(|_| {
        (
            EmailError::InvalidAddress,
            // The address itself is echoed: it is not a secret, and without it
            // the gamemode author cannot tell which of several recipients was
            // the bad one.
            format!("'{address}' is not a valid email address"),
        )
    })?;

    let name = name.trim();
    if name.is_empty() {
        return Ok(Mailbox::new(None, parsed));
    }

    check_header_value(name)?;
    Ok(Mailbox::new(Some(name.to_string()), parsed))
}

/// True when `address` is a syntactically valid mailbox. Backs
/// `email_is_valid_address`, so a gamemode can reject a typo at registration
/// instead of discovering it when the mail bounces.
pub fn is_valid_address(address: &str) -> bool {
    let address = address.trim();
    !address.contains(['\r', '\n', '\0']) && lettre::Address::from_str(address).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_value_is_accepted() {
        assert!(check_header_value("Welcome to the server").is_ok());
    }

    #[test]
    fn a_line_break_is_rejected() {
        for hostile in [
            "Subject\r\nBcc: attacker@evil.tld",
            "Subject\nBcc: attacker@evil.tld",
            "Subject\rX-Injected: 1",
        ] {
            let Err((code, _)) = check_header_value(hostile) else {
                panic!("expected {hostile:?} to be rejected");
            };
            assert_eq!(code, EmailError::HeaderInjection);
        }
    }

    #[test]
    fn a_nul_is_rejected() {
        assert!(check_header_value("Subject\0truncated").is_err());
    }

    #[test]
    fn an_overlong_value_is_rejected() {
        let long = "a".repeat(MAX_HEADER_LEN + 1);
        let Err((code, _)) = check_header_value(&long) else {
            panic!("expected an overlong value to be rejected");
        };
        assert_eq!(code, EmailError::HeaderInjection);
        assert!(check_header_value(&"a".repeat(MAX_HEADER_LEN)).is_ok());
    }

    #[test]
    fn header_names_reject_colons_spaces_and_emptiness() {
        assert!(check_header_name("X-Server-Name").is_ok());
        assert!(check_header_name("X-Bad: Value").is_err());
        assert!(check_header_name("X Bad").is_err());
        assert!(check_header_name("").is_err());
        assert!(check_header_name("X-Ação").is_err());
    }

    #[test]
    fn a_mailbox_parses_with_and_without_a_display_name() {
        let bare = parse_mailbox("player@example.com", "").expect("valid address");
        assert!(bare.name.is_none());

        let named = parse_mailbox("player@example.com", "Player One").expect("valid address");
        assert_eq!(named.name.as_deref(), Some("Player One"));
    }

    #[test]
    fn surrounding_whitespace_is_tolerated() {
        assert!(parse_mailbox("  player@example.com  ", "  Player  ").is_ok());
    }

    #[test]
    fn a_malformed_address_is_rejected_as_such() {
        for bad in ["not-an-address", "@example.com", "player@", ""] {
            let Err((code, _)) = parse_mailbox(bad, "") else {
                panic!("expected {bad:?} to be rejected");
            };
            assert_eq!(code, EmailError::InvalidAddress);
        }
    }

    #[test]
    fn an_injected_address_reports_injection_not_a_parse_error() {
        let Err((code, _)) = parse_mailbox("ok@example.com\r\nBcc: evil@evil.tld", "") else {
            panic!("expected the injected address to be rejected");
        };
        assert_eq!(code, EmailError::HeaderInjection);
    }

    #[test]
    fn an_injected_display_name_is_rejected_too() {
        // The realistic case: the address is fine, the *nickname* is hostile.
        let Err((code, _)) = parse_mailbox("ok@example.com", "Bob\r\nBcc: evil@evil.tld") else {
            panic!("expected the injected display name to be rejected");
        };
        assert_eq!(code, EmailError::HeaderInjection);
    }

    #[test]
    fn is_valid_address_agrees_with_the_parser() {
        assert!(is_valid_address("player@example.com"));
        assert!(is_valid_address("first.last+tag@sub.example.co.uk"));
        assert!(!is_valid_address("nope"));
        assert!(!is_valid_address("ok@example.com\r\nBcc: evil@evil.tld"));
    }
}
