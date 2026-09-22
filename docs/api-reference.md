# API reference

Every native and callback the plugin exposes, in the order the include
declares them. The pages linked from each section explain the reasoning; this
is the list.

The names here are the snake_case ones. Including `<email_samp_omp>` instead
gives the same API under open.mp's naming: `Email_Setup`, `Email_SendTo`,
`Email_SetVar`, and so on.

## Callbacks

See [Errors](errors.md).

| | What it does |
|---|---|
| `forward OnEmailSent(account, const recipient[], const callback[])` | Fired on every loaded script when a message leaves. |
| `forward OnEmailError(account, const recipient[], const callback[], const error[], errorid)` | Fired on every loaded script when a send fails for good - after the retries a temporary failure gets, never once per attempt. |

## Setup and accounts

See [Configuration](configuration.md).

| | What it does |
|---|---|
| `email_setup(const source[] = "")` | Reads the configuration and opens an account, in one line. |
| `email_connect(const host[], const user[], const password[], const settings[] = "")` | Opens an account from values supplied in code. |
| `bool:email_close(account = 0)` | Closes an account, dropping its pooled sessions and every unsent draft bound to it. |
| `bool:email_is_account(account = 0)` | Whether an account handle is still open. |
| `bool:email_status(account, dest[], dest_len = sizeof(dest))` | Writes "host:port MODE" into dest - e.g. "smtp.gmail.com:587 STARTTLS". |
| `bool:email_stats(account = 0, &queued = 0, &sent = 0, &failed = 0)` | How the mail queue is doing, and what it has done, since the account was opened: |
| `bool:email_test(account = 0, const callback[] = "", const format[] = "", {Float,_}:...)` | Opens a session, authenticates and closes it, to prove the configuration works. |

## Errors

See [Errors](errors.md).

| | What it does |
|---|---|
| `email_errno(account = 0)` | Last error code for an account. |
| `bool:email_error(account, dest[], dest_len = sizeof(dest))` | Writes the last error message into dest, usually the relay's own reply. |

## Sending in one line

See [Sending](sending.md).

| | What it does |
|---|---|
| `bool:email_send_to(const to[], const subject[], const body[], const callback[] = "", const format[] = "", {Float,_}:...)` | Sends a plain-text mail to one address on the default account. |
| `bool:email_send_file(const to[], const path[], const callback[] = "", const format[] = "", {Float,_}:...)` | Sends a template file to one address; the file carries the subject and the wording, so the gamemode carries neither: |

## Building a message

See [Sending](sending.md) and [Templates](templates.md).

| | What it does |
|---|---|
| `email_new(account = 0)` | Creates an empty message draft. |
| `bool:email_destroy(message)` | Discards a draft that will not be sent. |
| `bool:email_is_message(message)` | Whether a handle is still a live draft. |
| `bool:email_add_to(message, const address[], const name[] = "")` | Adds a visible recipient. |
| `bool:email_add_cc(message, const address[], const name[] = "")` | Adds a carbon-copy recipient, visible to everyone else on the message. |
| `bool:email_add_bcc(message, const address[], const name[] = "")` | Adds a blind carbon-copy recipient, hidden from the other recipients. |
| `bool:email_set_from(message, const address[], const name[] = "")` | Overrides the account's From: identity for this message only. |
| `bool:email_set_reply_to(message, const address[], const name[] = "")` | Sets where replies should go. |
| `bool:email_set_subject(message, const subject[])` | Sets the subject line. |
| `bool:email_set_body(message, const body[])` | Sets the plain-text body. |
| `bool:email_set_html(message, const html[])` | Sets the HTML body. |
| `bool:email_add_header(message, const name[], const value[])` | Adds a header the plugin has no native of its own for - X-Priority, List-Unsubscribe, a tracking id. |
| `bool:email_set_priority(message, priority)` | Sets where the message stands in the send queue. |
| `bool:email_attach(message, const path[], const filename[] = "", const mime[] = "")` | Attaches a file from disk. |
| `bool:email_attach_data(message, const data[], const filename[], const mime[] = "")` | Attaches text generated in the gamemode - a CSV export, a JSON dump - without writing it to disk first. |
| `bool:email_embed(message, const path[], const cid[] = "")` | Embeds an image for the HTML body to show inline instead of as a download: |
| `bool:email_set_template(message, const path[])` | Points the message at a template file - the wording, in a file you can edit without recompiling. |
| `bool:email_set_var(message, const key[], const value[], {Float,_}:...)` | Sets {key} for this message. |
| `bool:email_send(message, const callback[] = "", const format[] = "", {Float,_}:...)` | Hands the draft to a worker thread and returns immediately. |

## Utility

| | What it does |
|---|---|
| `bool:email_is_valid_address(const address[])` | Whether a string is a valid mailbox, using the same parser the send path uses. |
| `email_force_reload_templates()` | Forces every cached template to be read from disk again. |
| `email_limit(limit)` | A limit the plugin enforces, so the gamemode can check before it builds - splitting a mailing into batches of email_limit(EMAIL_LIMIT_RECIPIENTS), say, instead of hardcoding 100. |
| `bool:email_log(level)` | Sets the minimum log level at runtime, for both the console and logs/email.log. |

## env_samp

See [Configuration](configuration.md#a-env-through-env_samp-recommended).

| | What it does |
|---|---|
| `email_setup_env(const prefix[] = "smtp")` | Opens an account from .env keys read through env_samp. |

## Constants

| Group | Values |
|---|---|
| Priorities | `EMAIL_PRIORITY_LOW`, `EMAIL_PRIORITY_NORMAL`, `EMAIL_PRIORITY_HIGH` |
| Errors | `EMAIL_ERROR_*` — see [Errors](errors.md#the-codes) |
| Limits | `EMAIL_LIMIT_RECIPIENTS`, `EMAIL_LIMIT_HEADERS`, `EMAIL_LIMIT_ATTACHMENT_BYTES`, `EMAIL_LIMIT_DRAFTS`, `EMAIL_LIMIT_HEADER_LEN` |
| Log levels | `EMAIL_LOG_NONE`, `EMAIL_LOG_ERROR`, `EMAIL_LOG_WARNING`, `EMAIL_LOG_INFO`, `EMAIL_LOG_ALL` |
| Accounts | `EMAIL_EVERY_ACCOUNT`, for `email_stats` |
| Version | `EMAIL_SAMP_VERSION` |

`Email::Name(args)` declares a callback in one line, in place of the
`forward` + `public` pair.

