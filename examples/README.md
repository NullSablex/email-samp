# email_samp examples

**Start with [`00_ready_to_send.pwn`](00_ready_to_send.pwn).** It is a complete, working gamemode: fill in four lines with your SMTP details and you can send mail. It comes with [`00_ready_to_send.tpl`](00_ready_to_send.tpl), the mail itself, ready to reword.

The other files each answer one question, and you only need the one you are asking.

| File | Answers |
|---|---|
| [`00_ready_to_send.pwn`](00_ready_to_send.pwn) | "I just want to send email." Everything in one file |
| [`01_configuration.pwn`](01_configuration.pwn) | "Where do I keep the password?" `.env`, `smtp.ini`, environment, a second account |
| [`02_rich_messages.pwn`](02_rich_messages.pwn) | "I need HTML, attachments, Cc/Bcc, an image in the mail" |
| [`03_templates.pwn`](03_templates.pwn) | "I want the wording in a file, not in the code" - `.tpl` or a plain `.html` (optional) |
| [`04_errors.pwn`](04_errors.pwn) | "How do I know it failed, and why?" |
| [`05_registration.pwn`](05_registration.pwn) | "`/register` should mail a confirmation code" |
| [`06_bulk.pwn`](06_bulk.pwn) | "I want to mail every player without breaking anything" |

Commands such as `/testmail` trigger the code, so everything in these files is reachable and was compiled as written.

## Sending, in short

```pawn
email_send_to("player@example.com", "Welcome", "Your account is ready.");
```

It returns at once and the mail leaves in the background, so it is safe anywhere. Add a callback when the script has to react:

```pawn
email_send_to(address, "Welcome", body, "OnMailSent", "d", playerid);

Email::OnMailSent(success, playerid)
{
    // ...
}
```

The callback gets the result first, then the values listed after the format string. `Email::` comes with the include and is the same as writing `forward OnMailSent(success, playerid);` plus `public OnMailSent(success, playerid)`.

## What the plugin protects for you

Player input can end up in a mail, so the plugin checks it. You do not have to:

- **Headers:** a line break in a subject, name, address or custom header could add a hidden `Bcc:`, so the value is refused with `EMAIL_ERROR_HEADER_INJECTION`. The body may contain line breaks.
- **Addresses** are parsed properly when added. `email_is_valid_address` runs the same check, so a player can fix a typo right away.
- **HTML:** in a template's `[html]` section every `{value}` is escaped, so a nickname with `<script>` arrives as text. `email_set_html` sends your markup as written, so build player text into HTML through a template.
- **Files:** attachments, embedded images and templates must be inside the server folder. `..` is fine while the result stays inside; a symlink anywhere in the path is refused. A path built from a nickname cannot mail out `server.cfg`.
- **TLS** is required by default and the relay's certificate is always checked. Leave `tls_verify` on and use `tls_ca` for a relay with its own certificate.

## Also worth knowing

- **Specifiers do not escape anything.** `%d`, `%s` and `%f` only say which kind of value follows, as in `format`. Escaping is automatic and happens when the value lands in the `[html]` part. `%r` is the one way to turn it off, for markup you built yourself.
- **Templates are cached** and reload on their own when the file changes; `email_force_reload_templates()` forces it when a timestamp was restored.
- **`email_stats` is the queue view:** waiting, sent and failed, for one account or for all of them with `EMAIL_EVERY_ACCOUNT`.
- **Ask for the limits** with `email_limit(EMAIL_LIMIT_RECIPIENTS)` and friends instead of hardcoding them.
- **A file with nothing to fill in is one line:** `email_send_file(address, "mails/notice.html")`.
- **`smtp.dry_run = 1`** writes every mail to `logs/dry-run/` as an `.eml` and sends nothing, which is how to work on a gamemode without spending quota.
- **`OnEmailSent`** runs for every mail that leaves, as `OnEmailError` runs for every one that does not.
- **`email_send` consumes the message** once it is queued. If it returns false the message is still yours: send it later or `email_destroy` it.
- **Temporary failures are retried** before you hear about them, so a failure in your callback or in `OnEmailError` is final.
- **Priority:** `EMAIL_PRIORITY_HIGH` for mail a player is waiting for, `LOW` for mailings. Neither starves the other.
- **The first account opened is the default** wherever an account id is optional.
- **Nothing is kept on disk.** At shutdown the plugin waits up to 10 seconds for queued mail.

## Compiling

```bash
pawncc -i../include 00_ready_to_send.pwn
```

`01_configuration.pwn` also needs `env_samp.inc` on the include path. For open.mp naming, include `<email_samp_omp>` instead and write `Email_Setup`, `Email_SendTo`, and so on; `Email::` works with both. One include per script, never both.
