// 04_errors.pwn - knowing when something went wrong, and why.
//
// There are two moments a problem can show up:
//
//   1. Right away: the native returns false or 0 (bad address, no account,
//      full queue). Ask email_errno / email_error what happened.
//   2. Later, when the relay answers: OnEmailError runs, and so does your
//      send callback with success = 0.

#include <a_samp>
#include <email_samp>

public OnGameModeInit()
{
    // How much the plugin writes to the console and logs/email.log.
    email_log(EMAIL_LOG_WARNING);

    if (!email_setup())
    {
        // 0 is the slot for failures before an account exists.
        new reason[256];
        email_error(0, reason);
        printf("[email] setup failed (error %d): %s", email_errno(0), reason);
        return 1;
    }

    // email_setup does not connect. This does, in the background, to prove
    // the host, the TLS and the password work - better now than on the first
    // player's registration.
    email_test(0, "OnMailTested");
    return 1;
}

Email::OnMailTested(success)
{
    new status[128];
    email_status(0, status);
    printf("[email] %s: %s", status, success ? ("working") : ("NOT working, see OnEmailError"));
    return 1;
}

// 1. Right away.
SendChecked(playerid, const address[])
{
    // Whether mail is configured at all - worth checking before offering a
    // mail feature to the player.
    if (!email_is_account())
    {
        return SendClientMessage(playerid, -1, "This server does not send mail.");
    }

    if (email_send_to(address, "Hello", "Test message."))
    {
        return 1;
    }

    switch (email_errno())
    {
        case EMAIL_ERROR_INVALID_ADDRESS:
            SendClientMessage(playerid, -1, "That email address is not valid.");
        case EMAIL_ERROR_QUEUE_FULL:
            SendClientMessage(playerid, -1, "Mail is busy, try again in a minute.");
        default:
            SendClientMessage(playerid, -1, "Mail is unavailable right now.");
    }
    return 0;
}

// 2. Later. The include already declares this callback, so only the public
// is needed. It runs in every script, which lets a filterscript keep the log.
//
// Temporary failures (relay down, "try later") are retried before this runs
// (the retries setting), so an error here is final.
public OnEmailError(account, const recipient[], const callback[], const error[], errorid)
{
    printf("[email] account %d, <%s>, error %d: %s", account, recipient, errorid, error);

    switch (errorid)
    {
        case EMAIL_ERROR_AUTH_FAILED:
            print("[email] wrong user or password (Gmail wants an app password)");
        case EMAIL_ERROR_CONNECTION_FAILED:
            print("[email] relay unreachable - wrong host or port, or it is down");
        case EMAIL_ERROR_SEND_FAILED:
            print("[email] the relay refused the message (quota, recipient, content)");
    }
    return 1;
}

// The counterpart: every mail that leaves, whether or not the send passed a
// callback. Useful for a filterscript that keeps the mail log.
public OnEmailSent(account, const recipient[], const callback[])
{
    printf("[email] sent on account %d to <%s>", account, recipient);
    #pragma unused callback
    return 1;
}

public OnPlayerCommandText(playerid, cmdtext[])
{
    if (!strcmp(cmdtext, "/mail ", true, 6)) return SendChecked(playerid, cmdtext[6]);
    return 0;
}
