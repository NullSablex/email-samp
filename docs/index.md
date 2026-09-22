# email_samp

SMTP mail plugin for **SA-MP** and **Open Multiplayer**, written in Rust.

Send mail straight from the server — registration codes, password resets, staff alerts — with no PHP script, no external service and no abandoned plugin in between. The plugin speaks SMTP itself, over TLS, and every call returns immediately: the mail leaves on a worker thread and reports back through a Pawn callback.

The same binary loads on SA-MP and on Open Multiplayer, natively as a component or through legacy mode.

## The whole of the common case

```pawn
#include <a_samp>
#include <email_samp>

public OnGameModeInit()
{
    email_setup();
    email_send_to("player@example.com", "Welcome", "Your account is ready.");
    return 1;
}
```

`email_setup()` reads the configuration from the environment or from `smtp.ini`; `email_send_to` queues the mail and returns. Nothing blocks the server.

## Where to go next

| Page | What it answers |
|---|---|
| [Installation](installation.md) | Where the files go on SA-MP and on open.mp |
| [Configuration](configuration.md) | Where the password lives, and every setting |
| [Sending](sending.md) | One-liners, building a message, callbacks, priority |
| [Templates](templates.md) | The wording in a `.tpl` or `.html` file, with values |
| [Errors](errors.md) | Knowing what failed, and why |
| [Security](security.md) | What the plugin guards against, and what it leaves to you |
| [API reference](api-reference.md) | Every native and callback |

The [`examples/`](https://github.com/NullSablex/email-samp/tree/master/examples) folder has a working gamemode per topic. Start with `00_ready_to_send.pwn`: fill in four lines and it sends.

## What it does for you

- **Nothing blocks.** Sends run on worker threads; a slow relay never stalls the server.
- **One table of settings**, whatever source you keep them in.
- **Mail from a file**, edited without recompiling.
- **Paced for real providers**: rate limit, priorities, retries on temporary failures, a queue limit.
- **Safe with player input**: header injection refused, HTML escaped, attachments confined to the server folder.
