use std::path::Path;

use lettre::message::Mailbox;
use samp::args::Args;
use samp::native;
use samp::prelude::*;

use crate::address;
use crate::callback::CallbackInfo;
use crate::error::{EmailError, Fail};
use crate::message::{
    Attachment, AttachmentSource, MAX_ATTACHMENT_BYTES, MAX_HEADERS, MAX_RECIPIENTS, MessageDraft,
};
use crate::plugin::EmailPlugin;
use crate::sender::Priority;

impl EmailPlugin {
    /// `email_new(account = 0)` — a blank draft bound to that account.
    #[native(name = "email_new")]
    pub fn email_new(&mut self, amx: &Amx, account_id: i32) -> i32 {
        let account_id = self.resolve_account(account_id);
        if !self.accounts.exists(account_id) {
            let fail =
                EmailError::InvalidAccount.because("no account is open - call email_setup() first");
            self.report(account_id, fail, "New message");
            return 0;
        }
        let id = self.messages.create(account_id, amx.ident());
        if id == 0 {
            let fail = EmailError::BuildFailed.because(format!(
                "{} drafts are already open: a message that is not sent must be \
                 destroyed with email_destroy",
                crate::message::MAX_DRAFTS
            ));
            self.report(account_id, fail, "New message");
        }
        id
    }

    /// `email_destroy(message)` — discards a draft that will not be sent.
    #[native(name = "email_destroy")]
    pub fn email_destroy(&mut self, _amx: &Amx, message_id: i32) -> bool {
        self.messages.destroy(message_id)
    }

    /// `email_is_message(message)` — whether the handle is still a live draft.
    #[native(name = "email_is_message")]
    pub fn email_is_message(&mut self, _amx: &Amx, message_id: i32) -> bool {
        self.messages.get(message_id).is_some()
    }

    /// `email_set_from(message, const address[], const name[] = "")`
    #[native(name = "email_set_from")]
    pub fn email_set_from(
        &mut self,
        _amx: &Amx,
        id: i32,
        address: &AmxString,
        name: &AmxString,
    ) -> bool {
        let mailbox = mailbox(address, name);
        self.edit(id, |d| {
            d.from = Some(mailbox?);
            Ok(())
        })
    }

    /// `email_set_reply_to(message, const address[], const name[] = "")`
    #[native(name = "email_set_reply_to")]
    pub fn email_set_reply_to(
        &mut self,
        _amx: &Amx,
        id: i32,
        address: &AmxString,
        name: &AmxString,
    ) -> bool {
        let mailbox = mailbox(address, name);
        self.edit(id, |d| {
            d.reply_to = Some(mailbox?);
            Ok(())
        })
    }

    /// `email_add_to(message, const address[], const name[] = "")`
    #[native(name = "email_add_to")]
    pub fn email_add_to(
        &mut self,
        _amx: &Amx,
        id: i32,
        address: &AmxString,
        name: &AmxString,
    ) -> bool {
        let mailbox = mailbox(address, name);
        self.edit(id, |d| add_recipient(d, |d| &mut d.to, mailbox?))
    }

    /// `email_add_cc(message, const address[], const name[] = "")`
    #[native(name = "email_add_cc")]
    pub fn email_add_cc(
        &mut self,
        _amx: &Amx,
        id: i32,
        address: &AmxString,
        name: &AmxString,
    ) -> bool {
        let mailbox = mailbox(address, name);
        self.edit(id, |d| add_recipient(d, |d| &mut d.cc, mailbox?))
    }

    /// `email_add_bcc(message, const address[], const name[] = "")`
    #[native(name = "email_add_bcc")]
    pub fn email_add_bcc(
        &mut self,
        _amx: &Amx,
        id: i32,
        address: &AmxString,
        name: &AmxString,
    ) -> bool {
        let mailbox = mailbox(address, name);
        self.edit(id, |d| add_recipient(d, |d| &mut d.bcc, mailbox?))
    }

    /// `email_set_subject(message, const subject[])`
    #[native(name = "email_set_subject")]
    pub fn email_set_subject(&mut self, _amx: &Amx, id: i32, subject: &AmxString) -> bool {
        let subject = subject.to_string();
        self.edit(id, |d| set_subject(d, subject))
    }

    /// `email_set_body(message, const body[])` — the plain-text body.
    #[native(name = "email_set_body")]
    pub fn email_set_body(&mut self, _amx: &Amx, id: i32, body: &AmxString) -> bool {
        let body = body.to_string();
        self.edit(id, |d| {
            d.text = body;
            Ok(())
        })
    }

    /// `email_set_html(message, const html[])` — the HTML body.
    #[native(name = "email_set_html")]
    pub fn email_set_html(&mut self, _amx: &Amx, id: i32, html: &AmxString) -> bool {
        let html = html.to_string();
        self.edit(id, |d| {
            d.html = html;
            Ok(())
        })
    }

    /// `email_set_priority(message, priority)` — `EMAIL_PRIORITY_*`.
    #[native(name = "email_set_priority")]
    pub fn email_set_priority(&mut self, _amx: &Amx, id: i32, priority: i32) -> bool {
        self.edit(id, |d| {
            d.priority = Priority::from_i32(priority).ok_or_else(|| {
                EmailError::BuildFailed
                    .because(format!("{priority} is not an EMAIL_PRIORITY_* value"))
            })?;
            Ok(())
        })
    }

    /// `email_add_header(message, const name[], const value[])`
    #[native(name = "email_add_header")]
    pub fn email_add_header(
        &mut self,
        _amx: &Amx,
        id: i32,
        name: &AmxString,
        value: &AmxString,
    ) -> bool {
        let (name, value) = (name.to_string(), value.to_string());
        self.edit(id, |d| {
            if d.headers.len() >= MAX_HEADERS {
                return Err(EmailError::BuildFailed.because(format!(
                    "a message may not carry more than {MAX_HEADERS} headers"
                )));
            }
            address::check_header_name(&name).map_err(context("invalid header name"))?;
            address::check_header_value(&value)
                .map_err(context(&format!("invalid value for header '{name}'")))?;
            d.headers.push((name, value));
            Ok(())
        })
    }

    /// `email_attach(message, const path[], const filename[] = "", const mime[] = "")`
    #[native(name = "email_attach")]
    pub fn email_attach(
        &mut self,
        _amx: &Amx,
        id: i32,
        path: &AmxString,
        filename: &AmxString,
        mime: &AmxString,
    ) -> bool {
        let attachment = file_attachment(
            &path.to_string(),
            filename.to_string(),
            mime.to_string(),
            None,
        );
        self.edit(id, |d| push_attachment(d, attachment?))
    }

    /// `email_embed(message, const path[], const cid[] = "")`
    #[native(name = "email_embed")]
    pub fn email_embed(&mut self, _amx: &Amx, id: i32, path: &AmxString, cid: &AmxString) -> bool {
        let path = path.to_string();
        let mut cid = cid.to_string();
        if cid.is_empty() {
            cid = Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        let attachment = address::check_header_value(&cid)
            .map_err(context("invalid content id"))
            .and_then(|()| file_attachment(&path, String::new(), String::new(), Some(cid)));
        self.edit(id, |d| push_attachment(d, attachment?))
    }

    /// `email_attach_data(message, const data[], const filename[], const mime[] = "")`
    #[native(name = "email_attach_data")]
    pub fn email_attach_data(
        &mut self,
        _amx: &Amx,
        id: i32,
        data: &AmxString,
        filename: &AmxString,
        mime: &AmxString,
    ) -> bool {
        let bytes = data.to_string().into_bytes();
        let attachment = attachment(
            filename.to_string(),
            mime.to_string(),
            None,
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            AttachmentSource::Data(bytes),
        );
        self.edit(id, |d| push_attachment(d, attachment?))
    }

    /// `email_send(message, const callback[] = "", const format[] = "", {Float,_}:...)`
    #[native(name = "email_send", raw)]
    pub fn email_send(&mut self, _amx: &Amx, mut args: Args) -> bool {
        let id = args.next_arg::<i32>().unwrap_or(0);
        let callback = super::read_callback_args(&mut args);

        match self.messages.get(id).map(|m| m.account_id) {
            Some(account_id) => self.queue(account_id, Some(id), callback),
            None => self.invalid_message(id),
        }
    }

    /// `email_send_to(const to[], const subject[], const body[], const callback[] = "",
    #[native(name = "email_send_to", raw)]
    pub fn email_send_to(&mut self, amx: &Amx, mut args: Args) -> bool {
        let mut next_string = || {
            args.next_arg::<AmxString>()
                .map(|s| s.to_string())
                .unwrap_or_default()
        };
        let (to, subject, body) = (next_string(), next_string(), next_string());
        let callback = super::read_callback_args(&mut args);

        self.quick_send(amx, callback, |d| {
            add_recipient(d, |d| &mut d.to, address::parse_mailbox(&to, "")?)?;
            d.text = body;
            set_subject(d, subject)
        })
    }

    /// `email_send_file(const to[], const path[], const callback[] = "",
    #[native(name = "email_send_file", raw)]
    pub fn email_send_file(&mut self, amx: &Amx, mut args: Args) -> bool {
        let mut next_string = || {
            args.next_arg::<AmxString>()
                .map(|s| s.to_string())
                .unwrap_or_default()
        };
        let (to, path) = (next_string(), next_string());
        let callback = super::read_callback_args(&mut args);

        // Read before the draft exists, so a wrong path is reported as such
        // rather than as a failed send.
        let template = self.templates.get(Path::new(&path));

        self.quick_send(amx, callback, |d| {
            add_recipient(d, |d| &mut d.to, address::parse_mailbox(&to, "")?)?;
            d.template = Some(template?);
            Ok(())
        })
    }

    /// The one-line senders: a draft on the default account, filled in by
    fn quick_send(
        &mut self,
        amx: &Amx,
        callback: CallbackInfo,
        fill: impl FnOnce(&mut MessageDraft) -> Result<(), Fail>,
    ) -> bool {
        let id = self.email_new(amx, 0);
        if id == 0 {
            return false;
        }

        let account_id = self.resolve_account(0);
        if self.edit(id, fill) && self.queue(account_id, Some(id), callback) {
            return true;
        }

        self.messages.destroy(id);
        false
    }

    /// Applies `change` to a live draft, reporting a rejection against the
    pub(crate) fn edit(
        &mut self,
        id: i32,
        change: impl FnOnce(&mut MessageDraft) -> Result<(), Fail>,
    ) -> bool {
        let Some(draft) = self.messages.get_mut(id) else {
            return self.invalid_message(id);
        };
        let account_id = draft.account_id;
        match change(draft) {
            Ok(()) => true,
            Err(fail) => self.report(account_id, fail, &format!("Message {id}")),
        }
    }
}

fn mailbox(address: &AmxString, name: &AmxString) -> Result<Mailbox, Fail> {
    address::parse_mailbox(&address.to_string(), &name.to_string())
}

/// Prefixes a failure's detail with what was being set.
fn context(what: &str) -> impl Fn(Fail) -> Fail + '_ {
    move |(code, detail)| (code, format!("{what}: {detail}"))
}

fn set_subject(draft: &mut MessageDraft, subject: String) -> Result<(), Fail> {
    address::check_header_value(&subject).map_err(context("the subject is unusable"))?;
    draft.subject = subject;
    Ok(())
}

fn add_recipient(
    draft: &mut MessageDraft,
    field: fn(&mut MessageDraft) -> &mut Vec<Mailbox>,
    mailbox: Mailbox,
) -> Result<(), Fail> {
    if draft.recipient_count() >= MAX_RECIPIENTS {
        return Err(EmailError::BuildFailed.because(format!(
            "a message may not exceed {MAX_RECIPIENTS} recipients"
        )));
    }
    field(draft).push(mailbox);
    Ok(())
}

fn push_attachment(draft: &mut MessageDraft, attachment: Attachment) -> Result<(), Fail> {
    let total = draft.attachment_bytes().saturating_add(attachment.size);
    if total > MAX_ATTACHMENT_BYTES {
        return Err(EmailError::AttachmentFailed.because(format!(
            "the message's attachments would reach {total} bytes, over the \
             {MAX_ATTACHMENT_BYTES} byte limit"
        )));
    }
    draft.attachments.push(attachment);
    Ok(())
}

/// An attachment read from disk at send time. The path is checked now, so a
fn file_attachment(
    path: &str,
    mut filename: String,
    mime: String,
    cid: Option<String>,
) -> Result<Attachment, Fail> {
    let path = Path::new(path);
    let resolved = crate::sandbox::resolve(path, EmailError::AttachmentFailed)?;
    if filename.is_empty() {
        // The name the gamemode used, not the symlink's target.
        filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| String::from("attachment"));
    }

    // Its size is known now, so an oversized file fails here rather than
    // after the worker has read it.
    let size = std::fs::metadata(&resolved).map(|m| m.len()).map_err(|e| {
        EmailError::AttachmentFailed.because(format!("'{}' could not be read: {e}", path.display()))
    })?;

    attachment(filename, mime, cid, size, AttachmentSource::File(resolved))
}

fn attachment(
    filename: String,
    mime: String,
    cid: Option<String>,
    size: u64,
    source: AttachmentSource,
) -> Result<Attachment, Fail> {
    if filename.is_empty() {
        return Err(EmailError::AttachmentFailed.because("an attachment needs a filename"));
    }
    // The filename becomes a Content-Disposition parameter, so it is a header
    // value like any other.
    address::check_header_value(&filename).map_err(context("invalid attachment filename"))?;
    // Parsed now: "'imagem/png' is not a valid MIME type" belongs on the line
    // that wrote it, not in a send callback.
    crate::message::content_type(&mime, &filename)?;

    if size > MAX_ATTACHMENT_BYTES {
        return Err(EmailError::AttachmentFailed.because(format!(
            "'{filename}' is {size} bytes, over the {MAX_ATTACHMENT_BYTES} byte limit"
        )));
    }

    Ok(Attachment {
        filename,
        size,
        cid,
        mime,
        source,
    })
}
