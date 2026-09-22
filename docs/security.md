# Security

Mail built from player input is the interesting attack surface here, so the plugin checks what it can and is explicit about what it cannot.

## Headers

An email header ends at a bare CR or LF. A subject built as `format(subject, sizeof(subject), "Welcome, %s", nickname)`, with a nickname containing `\r\nBcc: attacker@evil.tld`, does not produce a strange subject — it produces a **new header**, and the server blind-copies every password-reset mail to the attacker.

So anything that becomes a header is **rejected** if it contains CR, LF or NUL: subjects, display names, addresses, custom header names and values, attachment filenames, and the subject a template renders. Rejected rather than stripped, with `EMAIL_ERROR_HEADER_INJECTION`, so the gamemode learns its input is hostile instead of quietly sending half a message.

A real player does not have a line break in their nickname. Treat it as an attempt, not a typo.

The **body is exempt on purpose**: line breaks are legitimate there, and they are encoded, so the body can never reach the header block.

## Addresses

Addresses are parsed by an RFC 5321 parser, never matched with a pattern. `email_is_valid_address` runs the same parser, so what it accepts is exactly what will not be rejected later:

```pawn
if (!email_is_valid_address(address))
{
    return SendClientMessage(playerid, -1, "That is not a valid email address.");
}
```

Worth calling at registration: a typo caught while the player is still on the server can be corrected; one caught at send time is a silent bounce hours later. Do not write a stricter check of your own — `first.last+tag@sub.example.co.uk` is valid and common.

## HTML

Values substituted into a template's `[html]` part are HTML-escaped, so a nickname containing `<script>` arrives as text. That is automatic; see [Templates](templates.md#escaping-and-the-one-way-out).

`email_set_html` sends your markup as written — it cannot tell your markup from a player's. Build player text into HTML through a template, or escape it yourself.

## Files

Attachments, embedded images and templates can end up **inside a mail**, so a path that escapes the server folder is not just a read: it is a way to mail a file out. A gamemode building `logs/<nickname>.txt` would otherwise send `server.cfg` to whoever asked.

The rule:

- `..` is plain path arithmetic and is fine while the result stays inside the server folder;
- no part of the path may be a **symlink** (or a Windows junction), wherever it points;
- it must end at a regular file.

The check runs when the native is called and again in the worker right before reading, so replacing a file with a symlink in between does not get past it. The configuration file and `tls_ca` are exempt: only the operator sets them, and they never become part of a mail.

## TLS

STARTTLS is **required**, never opportunistic. Anyone who can watch the connection can also strip the offer from the relay's greeting, after which an opportunistic client authenticates in the clear. Requiring the upgrade turns that attack into a failed send.

The trust store is the root bundle compiled into the plugin, not the operating system's. Public providers work unconfigured; a relay with an internal or self-signed CA needs `tls_ca` pointing at the CA in PEM form.

Never `tls_verify = 0` to get past a certificate error: it accepts any certificate, so anyone who can redirect the connection reads the password and every message. The plugin warns on the console every time it is off.

## Plaintext credentials

Sending the password over an unencrypted connection to a host that is **not** this machine is refused outright. It takes an explicit `allow_plaintext_auth = 1`, and then a warning on every start.

A relay on `localhost`, `127.x` or `::1` is allowed with a warning: there the session never reaches a network.

## Credentials and logs

Passwords, addresses and relay replies never reach the console — the console gets a short line and an error code, and the detail goes to `logs/email.log`. Configuration errors name the key and what was expected, never the value.

Keep `smtp.ini` and `.env` out of version control. `email_status` is safe to expose in a debug command: it prints `host:port MODE` and no credentials.

## Limits

They exist so a mistake fails on the line that made it, instead of becoming a memory problem or a session the relay drops:

| Limit | Value |
|---|---|
| Recipients per message (To + Cc + Bcc) | 100 |
| Custom headers per message | 50 |
| Attachment bytes per message | 25 MB |
| Open drafts across every script | 5000 |
| Length of one header value | 900 |

Ask for them with `email_limit(EMAIL_LIMIT_*)` rather than writing the numbers into the gamemode.
