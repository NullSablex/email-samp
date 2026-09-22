// 05_registration.pwn - a real flow: /register <email> mails a code,
// /confirm <code> checks it.

#include <a_samp>
#include <email_samp>

#define CODE_LENGTH     6
#define CODE_MINUTES    15

// Everything a player has pending, in one row per player.
enum E_PENDING
{
    pend_code[CODE_LENGTH + 1],
    pend_expires,
    bool:pend_sending
}
new Pending[MAX_PLAYERS][E_PENDING];

public OnGameModeInit()
{
    email_setup();
    return 1;
}

public OnPlayerConnect(playerid)
{
    Pending[playerid][pend_code][0] = EOS;
    Pending[playerid][pend_sending] = false;
    return 1;
}

Register(playerid, const address[])
{
    // Check the address while the player is still here to fix a typo.
    if (!email_is_valid_address(address))
    {
        return SendClientMessage(playerid, -1, "That is not a valid email address.");
    }
    if (Pending[playerid][pend_sending])
    {
        return SendClientMessage(playerid, -1, "Your code is already on its way.");
    }

    for (new i = 0; i < CODE_LENGTH; i++)
    {
        Pending[playerid][pend_code][i] = '0' + random(10);
    }
    Pending[playerid][pend_code][CODE_LENGTH] = EOS;
    Pending[playerid][pend_expires] = gettime() + CODE_MINUTES * 60;

    new name[MAX_PLAYER_NAME];
    GetPlayerName(playerid, name, sizeof(name));

    new msg = email_new();
    email_add_to(msg, address, name);
    email_set_template(msg, "templates/confirm.tpl");
    email_set_var(msg, "name", name);
    email_set_var(msg, "code", Pending[playerid][pend_code]);
    email_set_var(msg, "minutes", "%d", CODE_MINUTES);

    // The player is waiting at the screen for this: HIGH goes ahead of any
    // mailing in the queue and is never refused for a full queue.
    email_set_priority(msg, EMAIL_PRIORITY_HIGH);

    // The name travels with the playerid: if the player leaves and someone
    // else takes the slot before the mail is sent, the callback can tell.
    if (!email_send(msg, "OnCodeSent", "ds", playerid, name))
    {
        email_destroy(msg);
        return SendClientMessage(playerid, -1, "Mail is unavailable. Try again later.");
    }

    Pending[playerid][pend_sending] = true;
    return SendClientMessage(playerid, -1, "Sending your code...");
}

Email::OnCodeSent(success, playerid, const name[])
{
    new current[MAX_PLAYER_NAME];
    GetPlayerName(playerid, current, sizeof(current));
    if (!IsPlayerConnected(playerid) || strcmp(current, name))
    {
        return 1;   // they left, or the slot belongs to someone else now
    }

    Pending[playerid][pend_sending] = false;
    if (success)
    {
        return SendClientMessage(playerid, -1, "Code sent. Check your inbox and type /confirm <code>.");
    }

    // The reason is in OnEmailError and logs/email.log; the player only
    // needs to know it did not work.
    Pending[playerid][pend_code][0] = EOS;
    return SendClientMessage(playerid, -1, "We could not send the mail. Try again in a minute.");
}

Confirm(playerid, const code[])
{
    if (Pending[playerid][pend_code][0] == EOS)
    {
        return SendClientMessage(playerid, -1, "No code pending. Use /register first.");
    }
    if (gettime() > Pending[playerid][pend_expires])
    {
        Pending[playerid][pend_code][0] = EOS;
        return SendClientMessage(playerid, -1, "That code expired. Use /register again.");
    }
    // strcmp treats an empty string as equal to anything: check it first,
    // or "/confirm " with no code would pass.
    if (code[0] == EOS || strcmp(code, Pending[playerid][pend_code]))
    {
        return SendClientMessage(playerid, -1, "Wrong code.");
    }

    Pending[playerid][pend_code][0] = EOS;
    // ... mark the account as confirmed in your database ...
    return SendClientMessage(playerid, -1, "Account confirmed. Welcome!");
}

public OnPlayerCommandText(playerid, cmdtext[])
{
    if (!strcmp(cmdtext, "/register ", true, 10)) return Register(playerid, cmdtext[10]);
    if (!strcmp(cmdtext, "/confirm ", true, 9))   return Confirm(playerid, cmdtext[9]);
    return 0;
}
