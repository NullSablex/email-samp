# Sending

Every native here returns at once. The mail is handed to a worker thread and leaves in the background, so calling any of them from `OnPlayerConnect` is fine.

## One line

```pawn
email_send_to("player@example.com", "Welcome", "Your account is ready.");
```

Or, when the wording lives in a file and needs nothing filled in:

```pawn
email_send_file("player@example.com", "mails/maintenance.html");
```

Both use the default account and plain text (or whatever the file carries).

## Knowing whether it worked

Name a callback. It receives the result **first**, then the values you listed after the format string:

```pawn
email_send_to(address, "Welcome", body, "OnMailSent", "d", playerid);

Email::OnMailSent(success, playerid)
{
    SendClientMessage(playerid, -1, success ? ("Email sent.") : ("Email failed."));
    return 1;
}
```

`Email::` comes with the include and is shorthand for `forward OnMailSent(success, playerid);` plus `public OnMailSent(success, playerid)`. The pair works just as well.

The format letters are `d`/`i` for an integer, `f` for a float and `s` for a string — the extras only, never the `success` that comes first.

A callback is optional. A failure is also reported to [`OnEmailError`](errors.md), which every script sees.

## Building a message

The moment you need HTML, attachments, more than one recipient or a template:

```pawn
new msg = email_new();
email_add_to(msg, address, name);
email_set_subject(msg, "Weekly report");
email_set_body(msg, "Report attached.");
email_set_html(msg, "<h2>Weekly report</h2><p>Report attached.</p>");
email_attach(msg, "scriptfiles/report.csv");
email_send(msg, "OnMailSent", "d", playerid);
```

Setting both a text and an HTML body produces a `multipart/alternative` message: clients that render HTML use it, the rest fall back to the text. Sending HTML alone scores badly with spam filters, so write both — or use a [`.tpl` template](templates.md), which keeps the two together.

`email_send` **consumes the handle** once the message is queued: after that the id is dead. If it returns `false` the message is still yours — send it later, or `email_destroy` it.

### Recipients

`email_add_to` is visible, `email_add_cc` is visible to everyone else, and `email_add_bcc` is hidden. For mailing several players it has to be Bcc, or every player learns every other player's address. The cap is 100 across the three; ask for it with `email_limit(EMAIL_LIMIT_RECIPIENTS)` rather than writing the number down.

### Attachments and images

| Native | What it does |
|---|---|
| `email_attach` | a file from disk, offered as a download |
| `email_attach_data` | text built in the gamemode (a CSV export), with no file |
| `email_embed` | an image shown **inside** the HTML, as `cid:name` |

Path, size and MIME type are checked on the line that attaches, so a mistake does not wait for the send. The contents are read by the worker at send time, so a large file never blocks the server. See [Security](security.md#files) for where the files may live.

```pawn
email_embed(msg, "scriptfiles/logo.png");
email_set_html(msg, "<img src=\"cid:logo\"> Welcome!");
```

## Priority

```pawn
email_set_priority(msg, EMAIL_PRIORITY_HIGH);
```

| Level | For |
|---|---|
| `EMAIL_PRIORITY_HIGH` | a player is waiting: a confirmation code, a password reset |
| `EMAIL_PRIORITY_NORMAL` | the default |
| `EMAIL_PRIORITY_LOW` | mailings, newsletters — nobody is waiting |

Each level is a **head start**, not a separate queue: `HIGH` goes ahead of a `NORMAL` queued at the same moment, but a `LOW` that has waited ten minutes outranks a fresh `NORMAL`. So priorities reorder the queue without starving anything.

`LOW` may also fill only three quarters of the queue, so a mailing can never lock out a password reset, and `HIGH` is never refused for a full queue.

## Pacing

Three settings do the work that a gamemode would otherwise do with timers:

- **`rate_limit`** — messages per minute, evenly spaced (providers watch short windows, so a full minute's worth in the first second is exactly what they throttle). It holds back only its own account.
- **`retries`** — a temporary failure (a 4xx, or the relay unreachable) is tried again with a doubling delay. Your callback runs **once**, with the final outcome. When the relay itself is unreachable the whole account pauses instead, so an outage costs one probe per pause rather than one per queued mail.
- **`queue_limit`** — past it, a send fails on the calling line with `EMAIL_ERROR_QUEUE_FULL` instead of growing the queue without bound.

So a mailing is a plain loop:

```pawn
for (new i = 0; i < MAX_PLAYERS; i++)
{
    new address[128];
    if (!GetPlayerEmail(i, address)) continue;

    new msg = email_new();
    email_add_to(msg, address);
    email_set_subject(msg, "Announcement");
    email_set_body(msg, text);
    email_set_priority(msg, EMAIL_PRIORITY_LOW);

    if (email_send(msg)) continue;

    email_destroy(msg);
    if (email_errno() == EMAIL_ERROR_QUEUE_FULL) break;   // the rest next time
}
```

## Watching the queue

```pawn
new waiting, sent, failed;
email_stats(EMAIL_EVERY_ACCOUNT, waiting, sent, failed);
```

`waiting` is what still has to go out, including messages being retried. `email_test` passes through the queue but counts in neither `sent` nor `failed`: it is a check, not mail. Pass an account id, or `0` for the default one, to see just that account.

The three outputs have defaults, so you can ask for only the first one or two — `email_stats(0, waiting)` is a valid call. The `0` in the declaration is just what the hidden cell starts as; the plugin writes over it.

## Shutdown

When the server stops, the plugin waits up to ten seconds for queued mail to leave, then logs how many were lost. Nothing is kept on disk, so do not start a mailing right before a restart.
