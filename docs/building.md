# Building from source

## Requirements

- Rust stable with the targets `i686-unknown-linux-gnu` and `i686-pc-windows-msvc`
- `cargo-xwin`, to cross-compile the Windows `.dll` from Linux (the script installs it)
- **32-bit C support** for the Linux target: the TLS backend compiles C and 32-bit assembly, so `gcc -m32` must work — `apt install gcc-multilib g++-multilib` on Debian/Ubuntu, `glibc-devel.i686` on Fedora, `lib32-glibc` on Arch
- **LLVM**, only when cross-compiling the Windows `.dll` from Linux: archiving the TLS backend's objects for MSVC needs `llvm-lib`, which `cargo-xwin` does not ship. `scripts/build-linux.sh` finds it under `/usr/lib/llvm-*/bin`
- No mail or TLS system libraries: no PHPMailer, no `sendmail`, no OpenSSL

## Build

```bash
./scripts/build-linux.sh              # Linux .so + Windows .dll, into dist/
./scripts/build-windows.sh            # from Windows
PROFILE=dev ./scripts/build-linux.sh  # development build
```

The scripts read the plugin name from `Cargo.toml`, install any missing rustup target, and produce `dist/email_samp.so` and `dist/email_samp.dll`.

For a plain development build of one target:

```bash
cargo build --target i686-unknown-linux-gnu
cargo test --target i686-unknown-linux-gnu
cargo clippy --target i686-unknown-linux-gnu --all-targets -- -D warnings
```

## Why the dependencies look the way they do

Each choice below exists to keep the build pure Rust, because the plugin has to cross-compile to 32-bit Linux **and** 32-bit MSVC, where a C toolchain is exactly what nobody wants to set up.

### `samp` (rust-samp)

Comes from crates.io rather than the git tag: the published tree is the same one (v3.5.0 was tagged without bumping the version number, hence `3.4.0`), it builds with no clone, and Dependabot tracks a registry dependency properly.

Two features are on:

- `encoding` reads Pawn's 8-bit strings as the server's code page, so an accented nickname reaches the mail intact instead of arriving as `Jo?o`. See [`charset`](configuration.md#every-setting).
- `compression` gzips rotated log archives. It uses `flate2`, which is pure Rust.

### `lettre`

The SMTP client and MIME builder, with **`default-features = false`**. The default set pulls in `native-tls`, which binds OpenSSL and would make the i686/MSVC cross-build need a C toolchain and prebuilt libraries. The features kept are the pure-Rust equivalent:

| Feature | Why |
|---|---|
| `smtp-transport` | the SMTP client itself |
| `pool` | connection reuse, so an account keeps its session instead of paying TCP + TLS + AUTH per message |
| `builder` | MIME construction: multipart, attachments, RFC 2047 header encoding |
| `hostname` | the real machine name in EHLO; some relays reject `localhost` |
| `rustls-tls` | TLS via rustls |

`rustls-tls` implies `ring` and `webpki-roots`. `ring` rather than the default `aws-lc-rs` because it is the one that cross-compiles to i686, MSVC included.

!!! warning "The trust store is compiled in"
    `webpki-roots` means TLS verification uses the root bundle inside the binary, **not** the operating system's store. Public providers (Gmail, SendGrid, Mailgun, SES) work unconfigured; a relay with an internal or self-signed CA fails until [`tls_ca`](configuration.md#every-setting) points at that CA.

### `url` and `percent-encoding`

For parsing `smtps://user:pass@host:465`. Both are already in the tree through lettre, so naming them costs no extra compilation. Splitting the string by hand would get the percent-encoded userinfo wrong: a password containing `@`, `/` or `:` is only unambiguous once decoded.

### `encoding_rs`

What the SDK's `encoding` feature is built on. Naming it directly is what lets `charset = utf-8` exist alongside the Windows code pages.

### `fern`

Only for `Dispatch::into_log()` on the console sink the SDK hands back. Its version has to track the SDK's own.

## The generated include

`build.rs` writes `include/email_samp.inc` from `include/email_samp.inc.in`, replacing the version placeholder, and derives `include/email_samp_omp.inc` from it by rewriting every `email_*` name into open.mp's style. Edit the `.inc.in`; the other two are build output and are overwritten.
