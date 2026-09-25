# Changelog

All notable changes to this project are documented in this file.

Format inspired by [Keep a Changelog](https://keepachangelog.com/). Versioning follows [Semantic Versioning](https://semver.org/).

## [0.1.0] — 2026/09/22

First release: 34 natives, 2 callbacks and one binary that loads on SA-MP and on Open Multiplayer, natively as a component or through legacy mode.

### Sending

- **Nothing blocks.** Every native returns at once; mail leaves on a pool of four worker threads and reports back through a Pawn callback on a later tick. Nothing touches the network on the main thread.
- **`email_send_to(to, subject, body)`** — a plain-text mail to one address, in one line, on the default account.
- **`email_send_file(to, path)`** — a `.tpl` or `.html` file to one address, in one line; the file carries the subject and the wording.
- **`email_send(message)`** — the built message. It consumes the handle once queued; a refused send (closed account, full queue) leaves the handle alive to retry or destroy.
- **Callbacks get the result first**, then the values described by the format string. `Email::Name(args)` declares one in a line, in place of the `forward` + `public` pair.
- **`email_test`** opens a session, authenticates and closes it, so a wrong password shows up at startup instead of on the first player's registration.

### Messages

- **`email_new` plus the builder**: recipients (To, Cc, Bcc), sender identity, Reply-To, subject, plain and HTML bodies, custom headers.
- **Attachments** — `email_attach` for a file, `email_attach_data` for text built in the gamemode, `email_embed` for an image shown inside the HTML as `cid:name`. Path, size and MIME type are checked on the line that attaches; the contents are read by the worker at send time.
- **Both bodies make a `multipart/alternative`**, so clients that render HTML use it and the rest fall back to the text.

### Templates

- **The wording in a file**, edited without recompiling: a `.tpl` with `[subject]`, `[text]` and `[html]` sections, or a plain `.html` file whose `<title>` becomes the subject.
- **`email_set_var` takes every kind of value** — a template has no arithmetic and no conditions, so every value becomes text whatever it started as. Text is passed as written; a number needs `"%d"` only because Pawn cannot pass one where a string is expected. The only specifiers that decide anything are `%.Nf`, for the decimals a float keeps, and `%r`, which turns the automatic HTML escaping off.
- **Substitution is a single pass**: a value is never re-scanned, so one variable cannot be used to read another. An unknown marker stays visible in the mail.
- **Rendered at send time**, so `email_set_template` and `email_set_var` may be called in any order, and the template fills in only what the message left empty.
- **Cached per file** and reloaded when its size or mtime changes. `email_force_reload_templates` forces it for a deploy that restores timestamps.

### Configuration

- **One table of keys, 19 of them**, read the same way from every source: a `.env` through [env_samp](https://github.com/NullSablex/env-samp), the process environment, an SMTP URL, `smtp.ini`, or the settings string `email_connect` takes. Keys are case-insensitive and an optional `SMTP_` prefix is dropped.
- **An unrecognised key is an error**, never a silent no-op: `SMTP_PASSWD` would otherwise look exactly like "no password configured".
- **`email_setup()`** reads and opens in one line; the first account opened becomes the default, so no call site has to carry a handle. **`email_setup_env(prefix)`** does the same from `.env` keys, and another prefix opens another account.
- **`email_connect(host, user, password, settings)`** for credentials computed at runtime: separate arguments, so a password needs no escaping.

### Delivery

Sends go through a scheduler rather than a queue, and no worker ever sleeps holding a thread.

- **`email_set_priority`** with `EMAIL_PRIORITY_LOW`, `NORMAL` and `HIGH`. Each level is a head start, not a separate queue: `HIGH` goes first, and a `LOW` that has waited long enough still beats a fresh `NORMAL`, so nothing starves.
- **`rate_limit`** — messages per minute, evenly spaced, holding back only its own account.
- **`retries`** — a temporary failure is tried again with a doubling delay and the callback runs once, with the final outcome. An unreachable relay pauses the whole account instead (5 s, doubling up to 5 min), so an outage costs one probe per pause rather than one per queued mail.
- **`queue_limit`** — past it a send fails on the calling line with `EMAIL_ERROR_QUEUE_FULL`. `LOW` may fill only three quarters of it, so a mailing can never lock out a password reset, and `HIGH` is never refused by it.
- **Shutdown waits** up to 10 seconds for queued mail to leave, then says how many were lost. Nothing is kept on disk.
- **A panicking send does not take the worker with it**: it is reported as a failed send and the pool carries on.

### Security

- **Header injection is refused, not sanitised** — CR, LF or NUL in a subject, display name, address, custom header or attachment name is rejected with `EMAIL_ERROR_HEADER_INJECTION`, including in a subject a template rendered. Addresses are parsed by an RFC 5321 parser, never matched with a pattern.
- **Template values are HTML-escaped** in the `[html]` part, so a nickname containing `<script>` arrives as text. `%r` is the one way to turn that off, for markup the gamemode built itself.
- **Files stay in the server folder** — attachments, embedded images and templates must resolve inside the working directory (`..` is allowed while the result stays inside), and a symlink or junction anywhere in the path is refused. Checked when the native is called and again right before the worker reads, with the opened file's identity compared to the checked one.
- **TLS is required, not optional**: STARTTLS must succeed or the send fails. The trust store is the webpki bundle compiled into the binary, so a private CA needs `tls_ca`.
- **Plaintext credentials take an explicit opt-in** — sending the password unencrypted to a host other than this machine needs `allow_plaintext_auth=1`; a relay on localhost is allowed with a warning.
- **Caps that turn a leak into an error** — 100 recipients, 50 custom headers and 25 MB of attachments per message, 5000 open drafts, 900 bytes per header value.
- **Credentials never reach the console.** Passwords, addresses and relay replies go only to `logs/email.log`; configuration errors name the key and what was expected, never the value.

### Diagnostics

- **`email_errno` / `email_error`** — `0` is the default account, or the global slot while no account is open, which is where failures from before one existed land.
- **`OnEmailError`** fires once per message, after any retries, so an error there is final. **`OnEmailSent`** is its counterpart for mail that leaves. Both reach only the scripts that define them, worked out once when each script loads.
- **`email_stats`** — waiting, sent and failed, per account or for every account with `EMAIL_EVERY_ACCOUNT`.
- **`email_limit`** — the limits the plugin enforces, so a gamemode splits a mailing by `email_limit(EMAIL_LIMIT_RECIPIENTS)` instead of hardcoding 100.
- **`email_status`** writes `host:port MODE`, safe to show in a debug command.
- **`dry_run`** writes each message to `logs/dry-run/` as an `.eml` and sends nothing, for working on a gamemode without spending quota or mailing real players.
- **`logs/email.log`** rotates at 50 MB into gzipped archives; `email_log` changes the level at runtime.

### Platform

- **Accented names survive the trip** — Pawn's 8-bit strings are read as the server's code page (`charset`, any encoding name; `windows-1252` by default), instead of being assumed to be UTF-8 and arriving as `Jo?o`. Writing back, a character the code page cannot represent is reported instead of silently becoming `?`.
- **Two includes** — `<email_samp>` in snake_case and `<email_samp_omp>` in open.mp's `Prefix_PascalCase`, generated from the first so they cannot drift.
- **No external dependencies** — no PHPMailer, no `sendmail`, no OpenSSL. SMTP and TLS (rustls) are compiled into the binary, for Linux i686 and Windows i686 (MSVC).
- **Documentation** in English and Portuguese, and seven runnable examples covering every native.
