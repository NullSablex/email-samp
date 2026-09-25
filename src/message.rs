//! Message drafts: what the gamemode builds up before `email_send` takes it.
//!
//! Addresses are parsed when the native is called, so a typo is reported on
//! the line that added it, while the gamemode still knows which player it
//! belonged to. Attachment *contents* are read in the worker instead, that
//! being the one genuinely slow step.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use lettre::message::header::{ContentType, HeaderName, HeaderValue};
use lettre::message::{
    Attachment as LettreAttachment, Mailbox, MultiPart, MultiPartBuilder, SinglePart,
};
use samp::amx::AmxIdent;

use crate::error::{EmailError, Fail};

/// Recipients per message, across To + Cc + Bcc.
///
/// Relays impose their own limit (RFC 5321 guarantees only 100) and disconnect
/// when it is exceeded. Failing here instead gives a clear plugin-side error.
pub const MAX_RECIPIENTS: usize = 100;

/// Live drafts across every script.
///
/// A gamemode that creates drafts in a loop without sending or destroying
/// them would otherwise grow the map until the process dies. This turns that
/// into a clear error on the line that leaks.
pub const MAX_DRAFTS: usize = 5000;

/// Custom headers per message, beyond the ones the plugin writes itself.
pub const MAX_HEADERS: usize = 50;

/// Total attachment bytes per message, before base64 expansion.
///
/// 25 MB is the common ceiling among providers; base64 pushes that to ~34 MB
/// on the wire, which most of them still accept.
pub const MAX_ATTACHMENT_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone)]
pub enum AttachmentSource {
    /// Read from disk in the worker thread.
    File(PathBuf),
    /// Bytes supplied directly from Pawn.
    Data(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,
    /// Known when the native accepted it, so the per-message limit is checked
    /// on that line instead of minutes later in a worker.
    pub size: u64,
    /// Set for an image embedded in the HTML (`<img src="cid:logo">`) rather
    /// than offered as a download.
    pub cid: Option<String>,
    /// Empty means "guess from the filename extension".
    pub mime: String,
    pub source: AttachmentSource,
}

/// A template variable and whether it may go into HTML unescaped.
///
/// Escaping is the default and is automatic; `raw` is what `%r` sets, for
/// markup the gamemode built itself.
#[derive(Debug, Clone)]
pub struct Var {
    pub text: String,
    pub raw: bool,
}

impl From<&str> for Var {
    /// Escaped, which is the default everywhere but `%r`.
    fn from(text: &str) -> Self {
        Self {
            text: text.to_string(),
            raw: false,
        }
    }
}

/// A message under construction. Becomes a [`SendJob`] when `email_send` takes
/// it, at which point the handle is consumed.
#[derive(Debug, Clone)]
pub struct MessageDraft {
    pub account_id: i32,
    /// The script that created it, so `on_amx_unload` can reclaim handles a
    /// restarted gamemode will never destroy itself.
    pub owner: AmxIdent,
    /// Overrides the account's identity when set.
    pub from: Option<Mailbox>,
    pub reply_to: Option<Mailbox>,
    pub to: Vec<Mailbox>,
    pub cc: Vec<Mailbox>,
    pub bcc: Vec<Mailbox>,
    pub subject: String,
    pub text: String,
    pub html: String,
    pub headers: Vec<(String, String)>,
    pub attachments: Vec<Attachment>,
    pub vars: HashMap<String, Var>,
    /// Rendered at build time, not when it was set.
    ///
    /// That is what makes `email_set_template` and `email_set_var` order
    /// independent: rendering on the spot would silently ignore a variable
    /// set one line later. Holding an `Arc`
    /// rather than a path also means a template reloaded from disk cannot
    /// change a message already queued.
    pub template: Option<Arc<crate::template::Template>>,
    /// Place in the send queue, not a header: recipients never see it.
    pub priority: crate::sender::Priority,
}

impl MessageDraft {
    pub fn new(account_id: i32, owner: AmxIdent) -> Self {
        Self {
            account_id,
            owner,
            from: None,
            reply_to: None,
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: String::new(),
            text: String::new(),
            html: String::new(),
            headers: Vec::new(),
            attachments: Vec::new(),
            vars: HashMap::new(),
            template: None,
            priority: crate::sender::Priority::default(),
        }
    }

    /// What the attachments add up to so far.
    pub fn attachment_bytes(&self) -> u64 {
        self.attachments
            .iter()
            .fold(0u64, |total, a| total.saturating_add(a.size))
    }

    pub fn recipient_count(&self) -> usize {
        self.to.len() + self.cc.len() + self.bcc.len()
    }

    /// The first To: address, for error reporting. `OnEmailError` names one
    /// recipient so a log line is actionable; the full list is in the message.
    pub fn primary_recipient(&self) -> String {
        self.to
            .first()
            .or_else(|| self.cc.first())
            .or_else(|| self.bcc.first())
            .map(|m| m.email.to_string())
            .unwrap_or_default()
    }
}

/// Assembles the draft into a wire-ready message.
///
/// `default_from` is the account identity, used when the draft did not set its
/// own. Runs in the worker thread — this is where attachment files are read.
pub fn build(draft: &MessageDraft, default_from: &Mailbox) -> Result<lettre::Message, Fail> {
    if draft.recipient_count() == 0 {
        return Err((
            EmailError::BuildFailed,
            String::from("the message has no recipient (use email_add_to)"),
        ));
    }

    let from = draft.from.clone().unwrap_or_else(|| default_from.clone());

    // A template fills in only what nothing set, so `email_set_subject` wins.
    let (subject, text, html) = render_parts(draft)?;

    let mut builder = lettre::Message::builder().from(from).subject(&subject);

    for to in &draft.to {
        builder = builder.to(to.clone());
    }
    for cc in &draft.cc {
        builder = builder.cc(cc.clone());
    }
    for bcc in &draft.bcc {
        builder = builder.bcc(bcc.clone());
    }
    if let Some(reply_to) = &draft.reply_to {
        builder = builder.reply_to(reply_to.clone());
    }

    let body = build_body(draft, &text, &html)?;

    let mut message = match body {
        Body::Single(part) => builder.singlepart(part),
        Body::Multi(part) => builder.multipart(part),
    }
    .map_err(|e| EmailError::BuildFailed.because(e.to_string()))?;

    // After the build, because the typed builder cannot express an arbitrary
    // header name. Both halves were validated when the native stored them; a
    // name lettre still rejects is skipped rather than losing the whole mail.
    for (name, value) in &draft.headers {
        let Ok(header) = HeaderName::new_from_ascii(name.clone()) else {
            continue;
        };
        message
            .headers_mut()
            .insert_raw(HeaderValue::new(header, value.clone()));
    }

    Ok(message)
}

enum Body {
    Single(SinglePart),
    Multi(MultiPart),
}

/// Resolves subject/text/html, substituting the template where a field was
/// left empty.
///
/// The rendered subject is re-checked for line breaks: the template itself is
/// trusted, but a variable holding a player nickname is not, and this is the
/// point where a smuggled CRLF would otherwise become a new header.
fn render_parts(draft: &MessageDraft) -> Result<(String, String, String), Fail> {
    let Some(template) = &draft.template else {
        return Ok((
            draft.subject.clone(),
            draft.text.clone(),
            draft.html.clone(),
        ));
    };

    let pick = |explicit: &str, from_template: &str, html: bool| -> String {
        if explicit.is_empty() {
            crate::template::render(from_template, &draft.vars, html)
        } else {
            explicit.to_string()
        }
    };

    let subject = pick(&draft.subject, &template.subject, false);
    crate::address::check_header_value(&subject)
        .map_err(|(code, detail)| (code, format!("the rendered subject is unusable: {detail}")))?;

    Ok((
        subject,
        pick(&draft.text, &template.text, false),
        pick(&draft.html, &template.html, true),
    ))
}

fn build_body(draft: &MessageDraft, text: &str, html: &str) -> Result<Body, Fail> {
    let mut total = 0;
    let (inline, attached): (Vec<&Attachment>, Vec<&Attachment>) =
        draft.attachments.iter().partition(|a| a.cid.is_some());

    // Embedded images sit next to the HTML in a multipart/related, which is
    // where clients resolve `cid:` references.
    let html_part = if inline.is_empty() {
        Body::Single(SinglePart::html(html.to_string()))
    } else {
        if html.is_empty() {
            return Err(EmailError::BuildFailed
                .because("email_embed needs an HTML body to reference the image"));
        }
        let mut related = MultiPart::related().singlepart(SinglePart::html(html.to_string()));
        for attachment in inline {
            related = related.singlepart(load(attachment, &mut total)?);
        }
        Body::Multi(related)
    };

    // A message with neither body is legal but almost always a mistake, so it
    // is sent as an empty text part rather than as a bodiless message that
    // some clients render as blank and others reject.
    let content = match (text.is_empty(), html.is_empty()) {
        // Both: multipart/alternative, text first. Clients pick the last part
        // they can render, so ordering is what makes HTML win where supported
        // and text win where it is not.
        (false, false) => Body::Multi(append(
            MultiPart::alternative().singlepart(SinglePart::plain(text.to_string())),
            html_part,
        )),
        (true, false) => html_part,
        _ => Body::Single(SinglePart::plain(text.to_string())),
    };

    if attached.is_empty() {
        return Ok(content);
    }

    // With attachments the body becomes multipart/mixed wrapping whatever the
    // content turned out to be.
    let mut mixed = nest(MultiPart::mixed(), content);
    for attachment in attached {
        mixed = mixed.singlepart(load(attachment, &mut total)?);
    }
    Ok(Body::Multi(mixed))
}

/// Starts `builder` with `body` as its first part.
fn nest(builder: MultiPartBuilder, body: Body) -> MultiPart {
    match body {
        Body::Single(part) => builder.singlepart(part),
        Body::Multi(part) => builder.multipart(part),
    }
}

/// Adds `body` as the next part of `parent`.
fn append(parent: MultiPart, body: Body) -> MultiPart {
    match body {
        Body::Single(part) => parent.singlepart(part),
        Body::Multi(part) => parent.multipart(part),
    }
}

/// Reads one attachment into a MIME part, counting it against the per-message
/// size limit.
fn load(attachment: &Attachment, total: &mut u64) -> Result<SinglePart, Fail> {
    let bytes = match &attachment.source {
        // Read through the sandbox: it re-checks the path and refuses a
        // file that was swapped since the native accepted it.
        AttachmentSource::File(path) => crate::sandbox::read(path, EmailError::AttachmentFailed)?,
        AttachmentSource::Data(bytes) => bytes.clone(),
    };

    *total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
    if *total > MAX_ATTACHMENT_BYTES {
        return Err(EmailError::AttachmentFailed.because(format!(
            "attachments exceed the {MAX_ATTACHMENT_BYTES} byte limit for one message"
        )));
    }

    // The type was parsed when the attachment was added; this cannot fail.
    let content_type = content_type(&attachment.mime, &attachment.filename)?;

    let part = match &attachment.cid {
        Some(cid) => LettreAttachment::new_inline(cid.clone()),
        None => LettreAttachment::new(attachment.filename.clone()),
    };
    Ok(part.body(bytes, content_type))
}

/// Parses a MIME type, or works one out from the filename when it is empty.
///
/// Called when the attachment is added, so a bad type fails on that line.
pub fn content_type(mime: &str, filename: &str) -> Result<ContentType, Fail> {
    let mime = if mime.is_empty() {
        guess_mime(filename)
    } else {
        mime.to_string()
    };
    ContentType::parse(&mime).map_err(|e| {
        EmailError::AttachmentFailed.because(format!("'{mime}' is not a valid MIME type: {e}"))
    })
}

/// Maps a filename extension onto a MIME type.
///
/// Only the types a gamemode actually attaches — logs, screenshots, exported
/// data. Anything unrecognised becomes `application/octet-stream`, which every
/// client handles as "download this", so the table never has to be exhaustive.
pub fn guess_mime(filename: &str) -> String {
    let ext = filename
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();

    let mime = match ext.as_str() {
        "txt" | "log" | "ini" | "cfg" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html; charset=utf-8",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        _ => "application/octet-stream",
    };

    mime.to_string()
}

pub struct MessageManager {
    messages: HashMap<i32, MessageDraft>,
    next_id: i32,
}

impl MessageManager {
    pub fn new() -> Self {
        Self {
            messages: HashMap::new(),
            next_id: 1,
        }
    }

    /// Returns 0 when [`MAX_DRAFTS`] are already open.
    pub fn create(&mut self, account_id: i32, owner: AmxIdent) -> i32 {
        if self.messages.len() >= MAX_DRAFTS {
            return 0;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.messages
            .insert(id, MessageDraft::new(account_id, owner));
        id
    }

    pub fn get(&self, id: i32) -> Option<&MessageDraft> {
        self.messages.get(&id)
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut MessageDraft> {
        self.messages.get_mut(&id)
    }

    /// Removes and returns the draft. `email_send` uses this: a message is
    /// owned by exactly one send, so the handle dies with the call and cannot
    /// be mutated while the worker is serialising it.
    pub fn take(&mut self, id: i32) -> Option<MessageDraft> {
        self.messages.remove(&id)
    }

    pub fn destroy(&mut self, id: i32) -> bool {
        self.messages.remove(&id).is_some()
    }

    /// Drops every draft owned by an unloaded script.
    pub fn destroy_by_amx(&mut self, ident: AmxIdent) {
        self.messages.retain(|_, m| m.owner != ident);
    }

    /// Drops every draft bound to a closed account: it can never be sent.
    pub fn destroy_by_account(&mut self, account_id: i32) {
        self.messages.retain(|_, m| m.account_id != account_id);
    }

    /// Live draft count. Only the tests need it — the Pawn side tracks its own
    /// handles.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.messages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::parse_mailbox;

    /// `AmxIdent` is an opaque pointer wrapper with no public constructor
    /// other than `From<*mut AMX>`. A null pointer is never dereferenced — it
    /// is only ever compared — so it is a fine stand-in for "some script".
    fn test_owner() -> AmxIdent {
        AmxIdent::from(std::ptr::null_mut::<samp::raw::types::AMX>())
    }

    fn mailbox(address: &str) -> Mailbox {
        parse_mailbox(address, "").expect("valid address")
    }

    fn attachment(filename: &str, source: AttachmentSource) -> Attachment {
        let size = match &source {
            AttachmentSource::Data(bytes) => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            AttachmentSource::File(_) => 0,
        };
        Attachment {
            filename: filename.into(),
            size,
            cid: None,
            mime: String::new(),
            source,
        }
    }

    fn draft_with_recipient() -> MessageDraft {
        let mut draft = MessageDraft::new(1, test_owner());
        draft.to.push(mailbox("player@example.com"));
        draft
    }

    #[test]
    fn a_message_without_a_recipient_is_refused() {
        let draft = MessageDraft::new(1, test_owner());
        let Err((code, _)) = build(&draft, &mailbox("noreply@example.com")) else {
            panic!("expected a recipient-less message to be refused");
        };
        assert_eq!(code, EmailError::BuildFailed);
    }

    #[test]
    fn a_text_only_message_builds() {
        let mut draft = draft_with_recipient();
        draft.subject = "Hello".into();
        draft.text = "Body".into();

        let msg = build(&draft, &mailbox("noreply@example.com")).expect("builds");
        let wire = String::from_utf8(msg.formatted()).expect("utf-8");
        assert!(wire.contains("Subject: Hello"));
        assert!(wire.contains("To: player@example.com"));
        assert!(wire.contains("From: noreply@example.com"));
    }

    #[test]
    fn text_and_html_become_multipart_alternative() {
        let mut draft = draft_with_recipient();
        draft.text = "plain".into();
        draft.html = "<p>rich</p>".into();

        let msg = build(&draft, &mailbox("noreply@example.com")).expect("builds");
        let wire = String::from_utf8(msg.formatted()).expect("utf-8");
        assert!(wire.contains("multipart/alternative"));
        assert!(wire.contains("text/plain"));
        assert!(wire.contains("text/html"));
    }

    #[test]
    fn an_attachment_wraps_the_body_in_multipart_mixed() {
        let mut draft = draft_with_recipient();
        draft.text = "see attached".into();
        draft.attachments.push(attachment(
            "report.csv",
            AttachmentSource::Data(b"a,b\n1,2\n".to_vec()),
        ));

        let msg = build(&draft, &mailbox("noreply@example.com")).expect("builds");
        let wire = String::from_utf8(msg.formatted()).expect("utf-8");
        assert!(wire.contains("multipart/mixed"));
        assert!(wire.contains("report.csv"));
        assert!(wire.contains("text/csv"));
    }

    #[test]
    fn accents_survive_the_subject_and_the_body() {
        let mut draft = draft_with_recipient();
        draft.subject = "Confirmação da conta de João".into();
        draft.text = "Olá João, sua inscrição está pronta. Ação: /confirmar".into();

        let msg = build(&draft, &mailbox("noreply@example.com")).expect("builds");
        let wire = String::from_utf8(msg.formatted()).expect("utf-8");

        // The subject cannot carry raw 8-bit bytes, so it is encoded per
        // RFC 2047 rather than mangled or stripped.
        assert!(wire.contains("Subject: =?utf-8?"), "{wire}");
        assert!(!wire.contains("Confirma\u{e7}\u{e3}o da conta"));
        // The body carries its own charset and encoding, so the accents are
        // there to be decoded.
        assert!(wire.contains("charset=utf-8"));
        let body = wire.rsplit("\r\n\r\n").next().expect("body");
        assert!(!body.is_empty());
    }

    #[test]
    fn a_hostile_value_is_escaped_in_the_html_and_left_alone_in_the_text() {
        let mut draft = draft_with_recipient();
        draft
            .vars
            .insert("name".into(), "<script>alert(1)</script> & \"co\"".into());
        draft.template = Some(std::sync::Arc::new(crate::template::Template {
            subject: "Hi {name}".into(),
            text: "Hello {name}".into(),
            html: "<p>Hello {name}</p>".into(),
        }));

        let msg = build(&draft, &mailbox("noreply@example.com")).expect("builds");
        let wire = String::from_utf8(msg.formatted()).expect("utf-8");

        // The HTML part cannot run what the player typed...
        assert!(
            wire.contains(
                "<p>Hello &lt;script&gt;alert(1)&lt;/script&gt; &amp; &quot;co&quot;</p>"
            )
        );
        assert!(!wire.contains("<p>Hello <script>"));
        // ...while the text part keeps it as written, where it is harmless.
        assert!(wire.contains("Hello <script>alert(1)</script>"));
    }

    #[test]
    fn an_embedded_image_sits_next_to_the_html_in_multipart_related() {
        let mut draft = draft_with_recipient();
        draft.text = "fallback".into();
        draft.html = "<img src=\"cid:logo\">".into();
        draft.attachments.push(Attachment {
            cid: Some("logo".into()),
            ..attachment("logo.png", AttachmentSource::Data(b"png".to_vec()))
        });
        draft.attachments.push(attachment(
            "rules.txt",
            AttachmentSource::Data(b"x".to_vec()),
        ));

        let msg = build(&draft, &mailbox("noreply@example.com")).expect("builds");
        let wire = String::from_utf8(msg.formatted()).expect("utf-8");
        let (mixed, related, alternative) = (
            wire.find("multipart/mixed").expect("mixed"),
            wire.find("multipart/related").expect("related"),
            wire.find("multipart/alternative").expect("alternative"),
        );
        // mixed( alternative( plain, related( html, image ) ), attachment )
        assert!(mixed < alternative && alternative < related);
        assert!(wire.contains("Content-ID: <logo>"));
        assert!(wire.contains("rules.txt"));
    }

    #[test]
    fn an_embedded_image_without_html_is_refused() {
        let mut draft = draft_with_recipient();
        draft.text = "no html".into();
        draft.attachments.push(Attachment {
            cid: Some("logo".into()),
            ..attachment("logo.png", AttachmentSource::Data(b"png".to_vec()))
        });

        let Err((code, _)) = build(&draft, &mailbox("noreply@example.com")) else {
            panic!("expected an embed with no HTML to fail");
        };
        assert_eq!(code, EmailError::BuildFailed);
    }

    #[test]
    fn the_draft_from_overrides_the_account_identity() {
        let mut draft = draft_with_recipient();
        draft.from = Some(mailbox("alerts@example.com"));

        let msg = build(&draft, &mailbox("noreply@example.com")).expect("builds");
        let wire = String::from_utf8(msg.formatted()).expect("utf-8");
        assert!(wire.contains("From: alerts@example.com"));
        assert!(!wire.contains("From: noreply@example.com"));
    }

    #[test]
    fn a_custom_header_is_emitted() {
        let mut draft = draft_with_recipient();
        draft
            .headers
            .push(("X-Server-Name".into(), "NullSablex RP".into()));

        let msg = build(&draft, &mailbox("noreply@example.com")).expect("builds");
        let wire = String::from_utf8(msg.formatted()).expect("utf-8");
        assert!(wire.contains("X-Server-Name: NullSablex RP"));
    }

    #[test]
    fn oversized_attachments_are_refused() {
        let mut draft = draft_with_recipient();
        draft.text = "x".into();
        // Two chunks that only exceed the cap together, so the running total
        // is what catches it rather than any single attachment.
        let half = usize::try_from(MAX_ATTACHMENT_BYTES).unwrap_or(usize::MAX) / 2 + 1;
        for name in ["a.bin", "b.bin"] {
            draft
                .attachments
                .push(attachment(name, AttachmentSource::Data(vec![0u8; half])));
        }

        let Err((code, _)) = build(&draft, &mailbox("noreply@example.com")) else {
            panic!("expected oversized attachments to be refused");
        };
        assert_eq!(code, EmailError::AttachmentFailed);
    }

    #[test]
    fn a_missing_attachment_file_reports_the_path() {
        let mut draft = draft_with_recipient();
        draft.text = "x".into();
        draft.attachments.push(attachment(
            "nope.txt",
            AttachmentSource::File(PathBuf::from("/definitely/not/here.txt")),
        ));

        let Err((code, message)) = build(&draft, &mailbox("noreply@example.com")) else {
            panic!("expected a missing attachment to fail the build");
        };
        assert_eq!(code, EmailError::AttachmentFailed);
        assert!(message.contains("here.txt"));
    }

    #[test]
    fn mime_guessing_covers_the_common_cases_and_falls_back() {
        assert!(guess_mime("dump.log").starts_with("text/plain"));
        assert_eq!(guess_mime("shot.PNG"), "image/png");
        assert_eq!(guess_mime("archive.zip"), "application/zip");
        assert_eq!(guess_mime("weird.qqq"), "application/octet-stream");
        assert_eq!(guess_mime("noextension"), "application/octet-stream");
    }

    #[test]
    fn the_draft_cap_refuses_rather_than_growing_forever() {
        let mut mgr = MessageManager::new();
        for _ in 0..MAX_DRAFTS {
            assert_ne!(mgr.create(1, test_owner()), 0);
        }
        assert_eq!(mgr.create(1, test_owner()), 0);

        // Destroying one makes room again.
        assert!(mgr.destroy(1));
        assert_ne!(mgr.create(1, test_owner()), 0);
    }

    #[test]
    fn taking_a_message_consumes_the_handle() {
        let mut mgr = MessageManager::new();
        let id = mgr.create(1, test_owner());
        assert!(mgr.take(id).is_some());
        assert!(mgr.take(id).is_none());
        assert!(mgr.get(id).is_none());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn closing_an_account_drops_only_its_drafts() {
        let mut mgr = MessageManager::new();
        let owner = test_owner();
        let keep = mgr.create(2, owner);
        mgr.create(1, owner);
        mgr.destroy_by_account(1);
        assert_eq!(mgr.len(), 1);
        assert!(mgr.get(keep).is_some());
    }

    #[test]
    fn recipient_count_spans_to_cc_and_bcc() {
        let mut draft = draft_with_recipient();
        draft.cc.push(mailbox("cc@example.com"));
        draft.bcc.push(mailbox("bcc@example.com"));
        assert_eq!(draft.recipient_count(), 3);
        assert_eq!(draft.primary_recipient(), "player@example.com");
    }
}
