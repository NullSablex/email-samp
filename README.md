# email_samp

> SMTP mail plugin for SA-MP and Open Multiplayer, written in Rust — by [NullSablex](https://github.com/NullSablex)

[![CI](https://github.com/NullSablex/email-samp/actions/workflows/rust.yml/badge.svg)](https://github.com/NullSablex/email-samp/actions/workflows/rust.yml)
[![CodeQL](https://github.com/NullSablex/email-samp/actions/workflows/codeql.yml/badge.svg)](https://github.com/NullSablex/email-samp/actions/workflows/codeql.yml)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/NullSablex/email-samp/badge)](https://scorecard.dev/viewer/?uri=github.com/NullSablex/email-samp)
![License](https://img.shields.io/badge/license-GPL--3.0-blue)
![SA-MP](https://img.shields.io/badge/SA--MP-0.3.7+-orange)
![Open Multiplayer](https://img.shields.io/badge/Open%20Multiplayer-native%20%26%20legacy-orange)
![Language](https://img.shields.io/badge/Rust-2024%20edition-orange)
![Build](https://img.shields.io/badge/build-Linux%20%7C%20Windows-green)
![Architecture](https://img.shields.io/badge/arch-x86%20(32--bit)-lightgrey)
[![Release](https://img.shields.io/github/v/release/NullSablex/email-samp?label=release)](https://github.com/NullSablex/email-samp/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/NullSablex/email-samp/total?label=downloads)](https://github.com/NullSablex/email-samp/releases)
[![Stars](https://img.shields.io/github/stars/NullSablex/email-samp?style=flat)](https://github.com/NullSablex/email-samp/stargazers)
[![Issues](https://img.shields.io/github/issues/NullSablex/email-samp)](https://github.com/NullSablex/email-samp/issues)
[![Last commit](https://img.shields.io/github/last-commit/NullSablex/email-samp)](https://github.com/NullSablex/email-samp/commits)

## Overview

**email_samp** sends email straight from the server — registration codes, password resets, staff alerts — with no PHP script, no external service and no abandoned plugin in between. It speaks SMTP itself, over TLS, and every call returns immediately: the mail leaves on a worker thread and reports back through a Pawn callback.

The same binary loads on SA-MP and on Open Multiplayer — natively as a component (recommended) or via legacy mode.

> **Not affiliated.** This is an independent, community-maintained project. It is **not** affiliated with, endorsed by or sponsored by SA-MP, the open.mp (Open Multiplayer) project, or any email provider named in the documentation. Those names belong to their owners and appear here only to describe what this plugin works with.

### Highlights

- **Zero external dependencies** — no PHPMailer, no `sendmail`, no OpenSSL. SMTP and TLS (via rustls) are compiled into the binary.
- **Nothing blocks.** Sends run on a pool of worker threads, so the server keeps ticking while a slow relay takes its time.
- **Configuration from one place** — a `.env` (through [env_samp](https://github.com/NullSablex/env-samp)), the process environment, an SMTP URL or `smtp.ini`. One table of keys, whichever source you pick.
- **Mail from a file** — a `.tpl` carrying subject, text and HTML, or a plain `.html` like the ones you already send from PHP, with `{markers}` filled in from the gamemode. Edit the file and the next mail uses it, with no restart.
- **Paced for real providers** — messages per minute, priorities, retries on temporary failures, and a queue limit that refuses a send on the calling line instead of growing without bound.
- **Accented names arrive intact** — Pawn's 8-bit strings are read as the server's code page (`charset`), not assumed to be UTF-8.
- **Safe with player input** — line breaks in headers are refused, addresses are parsed, template values are HTML-escaped, and attachments cannot escape the server folder.
- **Visible from the gamemode** — `email_stats` says how many mails are waiting, sent and failed; `email_limit` reports the limits the plugin enforces.
- **A dry-run mode** that writes each message to `logs/dry-run/` as an `.eml` and sends nothing, for working on a gamemode without spending quota.

## Installation

1. Download `email_samp.so` / `email_samp.dll` and the include from the [latest release](https://github.com/NullSablex/email-samp/releases/latest).
2. Put the include in your `pawno/include` (or `qawno/include`) folder.
3. Install the binary:
   - **SA-MP** — drop it into `plugins/` and add it to `server.cfg`:
     ```
     plugins email_samp.so
     ```
     (or `email_samp.dll` on Windows)
   - **Open Multiplayer (native, recommended)** — drop the binary into `components/`. open.mp finds it on start; no `config.json` entry needed.
   - **Open Multiplayer (legacy)** — the same binary works as a legacy plugin: put it in `plugins/` and add it to `legacy_plugins` in `config.json`.

## Quick start

```pawn
#include <a_samp>
#include <email_samp>

public OnGameModeInit()
{
    // Reads SMTP_* from the environment, or smtp.ini next to server.cfg
    if (!email_setup())
    {
        print("[email] mail is not configured");
        return 1;
    }

    email_send_to("player@example.com", "Welcome", "Your account is ready.");
    return 1;
}
```

With the settings in a `.env`, include `<env_samp>` first and call `email_setup_env()` instead: it reads `smtp.host`, `smtp.user`, `smtp.password` and the rest.

To know whether it worked, name a callback. The result comes first, then the values you listed:

```pawn
email_send_to(address, "Welcome", body, "OnMailSent", "d", playerid);

Email::OnMailSent(success, playerid)
{
    SendClientMessage(playerid, -1, success ? ("Email sent.") : ("Email failed."));
    return 1;
}
```

`Email::` ships with the include and is shorthand for the usual `forward` + `public` pair.

## Documentation

Full documentation, in English and Portuguese, lives at **<https://email-samp.nullsablex.com/>** (Portuguese under [`/pt/`](https://email-samp.nullsablex.com/pt/)). The Markdown sources are in [docs/](docs/) — `mkdocs serve` from the repo root for a local preview.

| Page | What it answers |
|---|---|
| [Installation](docs/installation.md) | Where the files go on SA-MP and on open.mp |
| [Configuration](docs/configuration.md) | Where the password lives, and every setting |
| [Sending](docs/sending.md) | One-liners, building a message, callbacks, priority, pacing |
| [Templates](docs/templates.md) | The wording in a `.tpl` or `.html` file, with values |
| [Errors](docs/errors.md) | Knowing what failed, and why |
| [Security](docs/security.md) | What the plugin guards against, and what it leaves to you |
| [API reference](docs/api-reference.md) | Every native and callback |
| [Building from source](docs/building.md) | Requirements, and why the dependencies look the way they do |

## Examples

Start with [`examples/00_ready_to_send.pwn`](examples/00_ready_to_send.pwn): a complete gamemode where you fill in four lines with your SMTP details and can send, with the mail itself in [`00_ready_to_send.tpl`](examples/00_ready_to_send.tpl), ready to reword.

The rest of [`examples/`](examples/) covers one question each — where to keep the password, HTML and attachments, templates, error handling, a `/register` flow with a confirmation code, and mailing every player. See the [examples README](examples/README.md).

## Configuration

Every source takes the same keys — `host`, `user`, `password`, `port`, `encryption`, `from`, `from_name`, `timeout`, `pool_size`, `tls_ca`, `tls_verify`, `retries`, `rate_limit`, `queue_limit`, `dry_run`. Keys are case-insensitive and an optional `SMTP_` prefix is dropped, so `SMTP_HOST`, `smtp_host` and `host` are one key. An unrecognised key is an error, never a silent no-op.

Templates: [`examples/env.example`](examples/env.example) for a `.env`, [`examples/smtp.ini.example`](examples/smtp.ini.example) for a file.

## Building from source

### Requirements

- Rust stable with the targets `i686-unknown-linux-gnu` and `i686-pc-windows-msvc`
- `cargo-xwin` for cross-compiling the Windows `.dll` from Linux (installed by the script)
- **32-bit C support** for the Linux target: the TLS backend (`ring`) compiles C and 32-bit assembly, so `gcc -m32` must work — `apt install gcc-multilib g++-multilib` on Debian/Ubuntu, `glibc-devel.i686` on Fedora, `lib32-glibc` on Arch
- **LLVM**, only when cross-compiling the Windows `.dll` from Linux
- No system mail or TLS libraries

### Build

```bash
./scripts/build-linux.sh              # Linux .so + Windows .dll
./scripts/build-windows.sh            # from Windows
PROFILE=dev ./scripts/build-linux.sh  # development build
```

## Security

- **TLS is required, not optional.** STARTTLS must succeed or the send fails; an attacker who strips it gets a failed mail, not a plaintext password.
- **The trust store is compiled in** (webpki roots), not the system's. Public providers work unconfigured; a relay with a private CA needs `tls_ca`.
- **Header injection is refused, not sanitised.** A line break in a subject, display name, address or custom header would add a header — a hidden `Bcc:` on a password-reset mail — so the value is rejected.
- **Template values are HTML-escaped** in the `[html]` part, so a nickname containing `<script>` arrives as text.
- **Files stay in the server folder.** Attachments, embedded images and templates are resolved and refused if they leave it or go through a symlink, so a path built from player input cannot mail out `server.cfg`.
- **Plaintext credentials take an explicit opt-in.** Sending the password unencrypted to a host other than this machine is refused unless `allow_plaintext_auth=1`; a relay on localhost is allowed with a warning.
- **Credentials never reach the console.** Passwords, addresses and relay replies go to `logs/email.log` only; the console gets a short line and an error code.
- Keep `smtp.ini` and `.env` out of version control.

## License

This project is distributed under the [GNU General Public License v3.0](LICENSE). You can use, modify and redistribute it freely, provided derivative works are released under the same license with source code available.
