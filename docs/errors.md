# Errors

A problem shows up at one of two moments.

## Right away

The native returns `false` or `0` — a malformed address, no account, a full queue, a file that cannot be read. Ask what happened:

```pawn
if (!email_send_to(address, "Hello", body))
{
    switch (email_errno())
    {
        case EMAIL_ERROR_INVALID_ADDRESS:
            SendClientMessage(playerid, -1, "That email address is not valid.");
        case EMAIL_ERROR_QUEUE_FULL:
            SendClientMessage(playerid, -1, "Mail is busy, try again in a minute.");
        default:
            SendClientMessage(playerid, -1, "Mail is unavailable right now.");
    }
}
```

`email_error(account, dest)` writes the message behind the code, which is usually the relay's own reply.

`email_errno()` with no argument is **the last failure on any account** — which is also the only place a failed `email_setup` or `email_connect` can be read, since no account exists yet. Pass an account id for that account's own last failure.

## Later, when the relay answers

`OnEmailError` runs in every loaded script, and your send callback runs with `success = 0`. Use the callback to tell the player, and this to log the reason:

```pawn
public OnEmailError(account, const recipient[], const callback[], const error[], errorid)
{
    printf("[email] account %d, <%s>, error %d: %s", account, recipient, errorid, error);
    return 1;
}
```

It fires **once**, after any retries: a temporary failure is tried again before you hear about it, so an error here is final.

`OnEmailSent` is the counterpart, for every message that leaves — useful for a filterscript that keeps the mail log without the gamemode passing a callback to each send.

Both are broadcasts, but only to the scripts that **define** them: which scripts those are is worked out once when each script loads, not per message. A gamemode plus a dozen filterscripts costs nothing for the ones that do not define the callback.

Whatever you do write there runs on the main thread, once per message. Keep it to a log line; a mailing of five thousand runs it five thousand times.

## The codes

| Code | Means |
|---|---|
| `EMAIL_ERROR_NONE` | no error |
| `EMAIL_ERROR_INVALID_ACCOUNT` | the account was never opened, or is closed |
| `EMAIL_ERROR_INVALID_MESSAGE` | the message handle is unknown, or was already sent |
| `EMAIL_ERROR_TEMPLATE_FAILED` | the template file could not be read |
| `EMAIL_ERROR_INVALID_ADDRESS` | not a valid mailbox |
| `EMAIL_ERROR_CONNECTION_FAILED` | unreachable, refused, timed out, or TLS failed |
| `EMAIL_ERROR_AUTH_FAILED` | wrong credentials, or no mechanism in common |
| `EMAIL_ERROR_SEND_FAILED` | the relay took the session and refused the message |
| `EMAIL_ERROR_BUILD_FAILED` | the message could not be assembled, or hit a limit |
| `EMAIL_ERROR_ATTACHMENT_FAILED` | the file is unusable: unreadable, too large, bad MIME type |
| `EMAIL_ERROR_HEADER_INJECTION` | a line break in a value that becomes a header |
| `EMAIL_ERROR_CONFIG_FAILED` | the configuration is missing, unreadable or wrong |
| `EMAIL_ERROR_QUEUE_FULL` | the account's queue is full; the message was **not** consumed |

## What goes where

The console gets a short line with a code. Everything else — the recipient's address, the relay's wording — goes only to `logs/email.log`, which rotates at 50 MB into gzipped archives. `email_log(EMAIL_LOG_WARNING)` changes how much either gets, at runtime.

That split is deliberate: a console is read by whoever is nearby, and a bounce message routinely contains a player's address.

## Checking the setup at startup

`email_setup` does not dial anything — it builds the transport, and the first TCP + TLS + AUTH happens on the first send. To find out sooner:

```pawn
email_test(0, "OnMailTested");

Email::OnMailTested(success)
{
    new status[128];
    email_status(0, status);   // "smtp.gmail.com:587 STARTTLS"
    printf("[email] %s: %s", status, success ? ("working") : ("NOT working"));
    return 1;
}
```

`email_test` opens a session, authenticates and closes it, on a worker thread like every send. A wrong password shows up in the console while you are watching, instead of on the first player's registration.
