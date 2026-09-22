// 00_ready_to_send.pwn - working mail in one file.
//
// Copy this into your gamemode, fill in the four lines below, and you can
// send. Nothing else here needs changing, and no other example is required.

#include <a_samp>
#include <email_samp>

// ---------------------------------------------------------------------------
// FILL THIS IN
// ---------------------------------------------------------------------------
#define MAIL_HOST       "smtp.gmail.com"
#define MAIL_USER       "you@gmail.com"
#define MAIL_PASSWORD   "abcd efgh ijkl mnop"     // Gmail: an APP password
#define MAIL_FROM_NAME  "Los Santos RP"           // the name players will see
// ---------------------------------------------------------------------------
//
// Other providers: smtp.sendgrid.net (user is the word "apikey"),
// smtp.mailgun.org, email-smtp.<region>.amazonaws.com. Port and encryption
// are worked out for you.
//
// Putting the password in the gamemode is fine to get going. Before the
// server goes public, move these values to a file or a .env - see
// 01_configuration.pwn.

public OnGameModeInit()
{
    new settings[64];
    format(settings, sizeof(settings), "from_name=\"%s\"", MAIL_FROM_NAME);

    if (!email_connect(MAIL_HOST, MAIL_USER, MAIL_PASSWORD, settings))
    {
        print("[email] mail is off: check the settings at the top of this file");
        return 1;
    }

    // Optional, but useful: logs in now instead of failing on the first real
    // mail, so a wrong password shows up while you are watching the console.
    email_test(0, "OnMailChecked");
    return 1;
}

// Email:: comes with the include: it is the same as writing
//     forward OnMailChecked(success);
//     public  OnMailChecked(success)
Email::OnMailChecked(success)
{
    print(success ? ("[email] mail is working") : ("[email] mail is NOT working, see the lines above"));
    return 1;
}

// ---------------------------------------------------------------------------
// Sending
// ---------------------------------------------------------------------------
//
// This is the whole API you need. It returns at once - the mail is sent in
// the background, so it is safe to call anywhere, even in OnPlayerConnect.
//
//     email_send_to("player@example.com", "Subject", "The message text.");
//
// Text with values in it: build the body first, as you would for a chat
// message.

SendWelcomeMail(playerid, const address[])
{
    new name[MAX_PLAYER_NAME], body[256];
    GetPlayerName(playerid, name, sizeof(name));
    format(body, sizeof(body),
        "Hello %s,\n\nYour account on %s is ready.\n\nSee you in game!",
        name, MAIL_FROM_NAME);

    return email_send_to(address, "Your account is ready", body);
}

// The same mail, with the wording in a file instead of in the code.
//
// 00_ready_to_send.tpl sits next to this script and holds the subject, the
// text and an HTML version. Edit the file, save, and the next mail uses it -
// no recompile. Put it wherever you like inside the server folder and change
// the path below to match.
//
// Already have an .html mail, the kind you would send from PHP? Point the
// same call at it: its <title> becomes the subject and {markers} still work.

SendWelcomeMailFromFile(playerid, const address[])
{
    new name[MAX_PLAYER_NAME];
    GetPlayerName(playerid, name, sizeof(name));

    new msg = email_new();
    email_add_to(msg, address, name);
    email_set_template(msg, "00_ready_to_send.tpl");

    // Fills {name} and {server} in the file.
    email_set_var(msg, "name", name);
    email_set_var(msg, "server", MAIL_FROM_NAME);

    return email_send(msg);
}

// Same as the first one, but telling the player whether it worked. The callback gets the
// result first, then the values you listed after "d" (here: the playerid).

SendWelcomeMailWithReply(playerid, const address[])
{
    new name[MAX_PLAYER_NAME], body[256];
    GetPlayerName(playerid, name, sizeof(name));
    format(body, sizeof(body), "Hello %s, your account is ready.", name);

    return email_send_to(address, "Your account is ready", body,
        "OnWelcomeMailSent", "d", playerid);
}

Email::OnWelcomeMailSent(success, playerid)
{
    SendClientMessage(playerid, -1, success
        ? ("Email sent. Check your inbox.")
        : ("We could not send the email. Try again later."));
    return 1;
}

// ---------------------------------------------------------------------------
// Try it: /testmail your@address.com
// ---------------------------------------------------------------------------

public OnPlayerCommandText(playerid, cmdtext[])
{
    if (!strcmp(cmdtext, "/testmail ", true, 10))
    {
        new address[128];
        format(address, sizeof(address), "%s", cmdtext[10]);

        // Catches a typo while the player is still here to fix it.
        if (!email_is_valid_address(address))
        {
            return SendClientMessage(playerid, -1, "That is not a valid email address.");
        }
        return SendWelcomeMailWithReply(playerid, address);
    }

    if (!strcmp(cmdtext, "/testmail2", true))
    {
        return SendWelcomeMail(playerid, "player@example.com");
    }

    if (!strcmp(cmdtext, "/testmail3", true))
    {
        return SendWelcomeMailFromFile(playerid, "player@example.com");
    }
    return 0;
}

// When a mail fails, this runs with the reason. Keep it: it is the difference
// between "mail does not work" and "the relay says the password is wrong".
public OnEmailError(account, const recipient[], const callback[], const error[], errorid)
{
    printf("[email] failed for <%s> (error %d): %s", recipient, errorid, error);
    return 1;
}

public OnGameModeExit()
{
    email_close();
    return 1;
}
