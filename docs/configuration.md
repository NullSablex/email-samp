# Configuration

Every source takes the **same keys**. Keys are case-insensitive and an optional `SMTP_` prefix is dropped, so `SMTP_HOST`, `smtp_host` and `host` are one key. A key the plugin does not know is an **error**, never a silent no-op: `SMTP_PASSWD` would otherwise look exactly like "no password configured", and you would spend an evening on a `535` that had nothing to do with your password.

## Where to keep it

### A `.env`, through env_samp (recommended)

Include [env_samp](https://github.com/NullSablex/env-samp) **before** this plugin and the include grows one function:

```pawn
#include <env_samp>
#include <email_samp>

public OnGameModeInit()
{
    email_setup_env();
    return 1;
}
```

with the keys under a prefix, next to the rest of your server's settings:

```
smtp.host      = smtp.gmail.com
smtp.user      = bot@gmail.com
smtp.password  = "abcd efgh ijkl mnop"
smtp.from_name = Los Santos RP
```

A second account is just another prefix: `email_setup_env("smtp.staff")` reads `smtp.staff.host` and so on.

This plugin deliberately does **not** read `.env` itself. env_samp already does, and two parsers for one format would eventually disagree about a password with a `#` in it.

### Environment variables

Docker, systemd or CI set them and plain `email_setup()` finds them:

```yaml
environment:
  SMTP_URL: smtps://bot%40example.com:secret@smtp.gmail.com
```

### A URL

One string carries host, port, credentials and encryption, and query parameters carry the rest:

```pawn
email_setup("smtps://bot%40example.com:secret@smtp.gmail.com");
email_setup("smtp://user:pw@relay.local:2525/?tls=none&from_name=Los+Santos+RP");
```

The userinfo is percent-encoded, which is what makes a password containing `@`, `/` or `:` unambiguous. `%40` is `@`.

### `smtp.ini`

Next to `server.cfg`, for servers that want neither. `email_setup()` falls back to it; `email_setup("config/mail.ini")` names another path.

### In code

```pawn
email_connect("smtp.example.com", "alerts@example.com", "secret",
    "encryption=tls; from_name=Server Alerts; rate_limit=30");
```

The credentials are separate arguments, so a password needs no escaping; everything else uses the same keys, separated by `;`.

## Discovery order

`email_setup()` with no argument tries the **process environment** first, then **`smtp.ini`**. The environment wins, so a container's `SMTP_URL` is not overridden by an `smtp.ini` baked into the image. The console says which source was used.

## Every setting

| Key | Meaning | Default |
|---|---|---|
| `url` | `smtps://user:pass@host:465` — carries everything at once | |
| `host` | relay hostname (required, unless `url`) | |
| `user` | SMTP username (alias `username`) | |
| `password` | SMTP password (alias `pass`) | |
| `port` | | follows `encryption` |
| `encryption` | `none`, `starttls` or `tls` | `starttls` |
| `from` | `From:` address | the username |
| `from_name` | display name recipients see | |
| `helo` | EHLO name | this machine's hostname |
| `timeout` | seconds | `30` |
| `pool_size` | pooled SMTP sessions | `4` |
| `tls_ca` | PEM root certificate, for a private CA | |
| `tls_verify` | verify the relay's certificate | `1` |
| `retries` | extra attempts after a *temporary* failure | `2` |
| `rate_limit` | messages per minute, evenly spaced; `0` unlimited | `0` |
| `queue_limit` | messages waiting at once | `1000` |
| `charset` | how Pawn's 8-bit strings are read; any encoding name | `windows-1252` |
| `allow_plaintext_auth` | send the password unencrypted to a remote host | `0` |
| `dry_run` | write to `logs/dry-run/` and send nothing | `0` |

Booleans accept `1`/`0`, `true`/`false`, `yes`/`no`, `on`/`off`.

### charset

SA-MP has no notion of UTF-8: a nickname with an accent is a byte in the server's code page. Reading it as the wrong one is what turns *João* into `Jo?o` or `JoÃ£o` in the mail.

Any encoding name works, with the usual aliases: `windows-1252` (the default, and what SA-MP uses in most of the world), `windows-1251` for Cyrillic, `windows-1250`, `1253`, `1254`, `1256`, `1257`, `iso-8859-2`, or `utf-8` for a gamemode that already stores UTF-8. An unknown name is refused at startup, naming the key. The setting is process-wide.

Going the other way — a relay reply written back into a Pawn buffer — the conversion can lose characters the code page has no room for, a Cyrillic message on a Windows-1252 server being the obvious case. The plugin says so in the console when that happens, and the full text is in `logs/email.log`.

### rate_limit, retries and queue_limit

These are what make a mailing safe to write as a plain loop; [Sending](sending.md#pacing) explains how they interact with priorities.

### dry_run

Writes every message to `logs/dry-run/` as an `.eml` file and sends nothing — no quota spent, no real player mailed by accident. The file is the real thing, headers and attachments included, so it opens in any mail client.

## Provider notes

| Provider | Host | Note |
|---|---|---|
| Gmail / Workspace | `smtp.gmail.com` | needs an **app password**, not the account password; quote it to keep the spaces |
| SendGrid | `smtp.sendgrid.net` | the user is the literal word `apikey` |
| Mailgun | `smtp.mailgun.org` | user is `postmaster@mg.yourdomain.com` |
| Amazon SES | `email-smtp.<region>.amazonaws.com` | SMTP credentials from the SES console, not your AWS keys |
| Local Postfix | `127.0.0.1` | the one honest use of `encryption=none` |

Port and encryption are worked out from each other, so setting `host` and the credentials is usually all it takes.

## Several accounts

The **first** account opened becomes the default: every native that takes an account uses it when you pass nothing, and a filterscript opening its own account cannot redirect the gamemode's mail. Address any other account by the id it returned.

```pawn
new Mail[2];
Mail[0] = email_setup_env();              // players
Mail[1] = email_setup_env("smtp.staff");  // alerts, another sender

new msg = email_new(Mail[1]);
```
