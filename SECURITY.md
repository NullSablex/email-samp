# Security Policy — email_samp

## Reporting a vulnerability

Found a security vulnerability? Please do not open a public issue.

**Contact:** open a private [Security Advisory](https://github.com/NullSablex/email-samp/security/advisories/new)
on GitHub, or e-mail the maintainer directly.

Expected response within **7 business days**.

---

## Scope

This policy covers the plugin in `NullSablex/email-samp`: the Rust source, the
generated Pawn includes and the examples.

A plugin that sends mail on behalf of a game server sits between untrusted
input (whatever a player typed) and a credential (the SMTP password), so the
findings that matter most here are:

- **Header injection** — any value that becomes a header and is not refused
  when it carries CR, LF or NUL: subject, display name, address, custom header,
  attachment filename, or a subject a template rendered.
- **TLS** — a session that continues unencrypted when `encryption` asks for
  STARTTLS, a certificate accepted without verification while `tls_verify` is
  on, or a hostname that is not checked.
- **Credential exposure** — the SMTP password, or a value carrying it, reaching
  the console, `logs/email.log`, an error message, a panic or a `.eml` written
  by `dry_run`.
- **Path escape** — an attachment, an embedded image or a template read from
  outside the server's working directory, or through a symlink.
- **Template rendering** — a value substituted into the `[html]` part without
  escaping, or one value reaching the content of another.

Out of scope: a relay's own behaviour, a provider rejecting or filtering mail,
and gamemode code built on top of the plugin — including a gamemode that turns
a protection off (`tls_verify = 0`, `allow_plaintext_auth = 1`, `%r`), which
the plugin documents as dangerous and requires you to opt into.

---

## Supported versions

Only the most recent [release](https://github.com/NullSablex/email-samp/releases)
receives security fixes.

---

## Dependencies

SMTP and TLS are compiled into the binary: the plugin loads no system mail or
TLS library, and no OpenSSL. TLS is [rustls](https://github.com/rustls/rustls),
verifying against the webpki root bundle that ships inside the binary rather
than the operating system's store; SMTP and MIME are
[lettre](https://github.com/lettre/lettre) with `default-features = false`,
which is what keeps the whole dependency tree pure Rust.

Advisories are tracked by Dependabot, which watches transitive crates as well,
and by `cargo audit` in CI — on every pull request and again weekly, because an
advisory against a crate already in `Cargo.lock` arrives on the RustSec
database's clock, not on ours.
