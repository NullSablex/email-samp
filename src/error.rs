//! Plugin-level error codes, mirrored in `include/email_samp.inc` as the
//! `EMAIL_ERROR_*` enum.
//!
//! An SMTP reply is a three-digit code plus free text, and the text is the
//! interesting part — so `email_errno` reports the plugin's classification
//! and `email_error` carries the relay's own words.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum EmailError {
    Ok = 0,
    /// An account handle that was never created, or already closed.
    InvalidAccount = 1,
    /// A message handle that was never created, or already sent/destroyed.
    InvalidMessage = 2,
    /// A template file that could not be read.
    TemplateFailed = 3,
    /// An address that is not a valid RFC 5321 mailbox.
    InvalidAddress = 4,
    /// TCP or TLS handshake with the relay failed.
    ConnectionFailed = 5,
    /// The relay rejected the credentials, or offered no mechanism we support.
    AuthFailed = 6,
    /// The relay accepted the session but refused the message.
    SendFailed = 7,
    /// The message could not be assembled (no recipient, no sender, ...).
    BuildFailed = 8,
    /// An attachment could not be read, or its MIME type was unparseable.
    AttachmentFailed = 9,
    /// A CR or LF was found in a field that becomes a header. See
    /// [`crate::address`].
    HeaderInjection = 10,
    /// The configuration was missing, unreadable, or had a bad value.
    ConfigFailed = 11,
    /// The account already has `queue_limit` messages waiting.
    QueueFull = 12,
}

/// A failure with its explanation: the code the gamemode branches on and the
/// text that goes to `email_error` and the log file.
pub type Fail = (EmailError, String);

impl EmailError {
    pub fn code(self) -> i32 {
        self as i32
    }

    /// Pairs the code with its explanation.
    pub fn because(self, detail: impl Into<String>) -> Fail {
        (self, detail.into())
    }
}

/// The last error recorded for one account (or globally, for failures that
/// happen before an account exists).
#[derive(Debug, Clone)]
pub struct ErrorState {
    pub code: EmailError,
    pub message: String,
}

impl ErrorState {
    pub fn ok() -> Self {
        Self::new(EmailError::Ok, "")
    }

    pub fn new(code: EmailError, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<Fail> for ErrorState {
    fn from((code, message): Fail) -> Self {
        Self { code, message }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_match_the_inc_enum() {
        // These numbers are API: EMAIL_ERROR_* in include/email_samp.inc.
        let expected = [
            (EmailError::Ok, 0),
            (EmailError::InvalidAccount, 1),
            (EmailError::InvalidMessage, 2),
            (EmailError::TemplateFailed, 3),
            (EmailError::InvalidAddress, 4),
            (EmailError::ConnectionFailed, 5),
            (EmailError::AuthFailed, 6),
            (EmailError::SendFailed, 7),
            (EmailError::BuildFailed, 8),
            (EmailError::AttachmentFailed, 9),
            (EmailError::HeaderInjection, 10),
            (EmailError::ConfigFailed, 11),
            (EmailError::QueueFull, 12),
        ];
        for (error, code) in expected {
            assert_eq!(error.code(), code, "{error:?}");
        }
    }

    #[test]
    fn the_include_declares_every_code_in_order() {
        // The test above pins the Rust side; this pins the Pawn side to it, so
        // renumbering one without the other fails here instead of in a
        // gamemode that branches on the wrong code.
        let inc = include_str!("../include/email_samp.inc.in");
        let start = inc.find("EMAIL_ERROR_NONE = 0").expect("error enum");
        let names: Vec<&str> = inc[start..]
            .lines()
            .map(str::trim)
            .take_while(|l| !l.starts_with('}'))
            .filter(|l| l.starts_with("EMAIL_ERROR_"))
            .collect();
        assert_eq!(names.len(), 13, "{names:?}");
        assert!(names[12].starts_with("EMAIL_ERROR_QUEUE_FULL"));
    }

    #[test]
    fn ok_state_carries_no_message() {
        let state = ErrorState::ok();
        assert_eq!(state.code, EmailError::Ok);
        assert!(state.message.is_empty());
    }

    #[test]
    fn a_failure_converts_into_a_state() {
        let state = ErrorState::from(EmailError::AuthFailed.because("535 authentication failed"));
        assert_eq!(state.code, EmailError::AuthFailed);
        assert_eq!(state.message, "535 authentication failed");
    }
}
