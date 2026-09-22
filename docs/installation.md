# Installation

Download `email_samp.so` (Linux) or `email_samp.dll` (Windows) and the include from the [latest release](https://github.com/NullSablex/email-samp/releases/latest). The plugin has no system dependencies: no PHPMailer, no `sendmail`, no OpenSSL.

## The include

Put `email_samp.inc` in `pawno/include` (SA-MP) or `qawno/include` (open.mp), then:

```pawn
#include <email_samp>
```

The release also carries `email_samp_omp.inc`, the same API under open.mp's naming (`Email_Setup`, `Email_SendTo`). Pick one per script, never both.

## SA-MP

Put the binary in `plugins/` and name it in `server.cfg`:

```
plugins email_samp.so
```

On Windows, `plugins email_samp.dll`.

## Open Multiplayer (native)

Put the binary in `components/`. open.mp finds it on start, with no entry in `config.json`. This is the recommended path: the plugin then uses open.mp's own APIs for timers and logging.

## Open Multiplayer (legacy)

The same binary also works as a legacy plugin. Put it in `plugins/` and add it to `legacy_plugins` in `config.json` — this one does need to be declared, or open.mp skips it.

## Checking it loaded

The console prints a banner with the version on start. To prove the credentials work as well, without waiting for the first real mail:

```pawn
public OnGameModeInit()
{
    email_setup();
    email_test(0, "OnMailTested");
    return 1;
}

Email::OnMailTested(success)
{
    printf("[email] %s", success ? ("mail works") : ("mail does NOT work"));
    return 1;
}
```

Details of any failure go to `logs/email.log`; the console gets a short line and an error code.

## Building from source

Requires Rust stable with the `i686-unknown-linux-gnu` and `i686-pc-windows-msvc` targets, `gcc-multilib`/`g++-multilib` for the 32-bit C parts of the TLS backend, and LLVM only when cross-compiling the Windows `.dll` from Linux.

```bash
./scripts/build-linux.sh              # Linux .so + Windows .dll
./scripts/build-windows.sh            # from Windows
PROFILE=dev ./scripts/build-linux.sh  # development build
```
