// 06_bulk.pwn - mailing many players without hurting the mail that matters.
//
// The plugin paces everything itself. Set once in the configuration:
//
//     smtp.rate_limit  = 60     at most 60 mails a minute, evenly spaced
//     smtp.queue_limit = 1000   more than this waiting and sends are refused
//     smtp.retries     = 2      temporary failures are tried again
//
// so a mailing is a plain loop.

#include <a_samp>
#include <email_samp>

public OnGameModeInit()
{
    email_setup();
    return 1;
}

// One message, many hidden recipients: the cheapest option when everybody
// gets the same text. Ask the plugin for the cap instead of writing 100 into
// the gamemode.
SendAnnouncement(const text[])
{
    new msg = email_new();
    email_add_to(msg, "noreply@example.com", "Los Santos RP");

    new address[128], room = email_limit(EMAIL_LIMIT_RECIPIENTS) - 1;   // -1: the To
    for (new i = 0; i < MAX_PLAYERS && room > 0; i++)
    {
        if (GetPlayerEmail(i, address) && email_add_bcc(msg, address)) room--;
    }

    email_set_subject(msg, "Announcement");
    email_set_body(msg, text);
    email_set_priority(msg, EMAIL_PRIORITY_LOW);
    return email_send(msg);
}

// One message per player, when the text is personal.
//
// LOW priority is what keeps this harmless: it waits behind anything a player
// is waiting for (a confirmation code), may use only three quarters of the
// queue, and still goes out - a LOW mail that waited 10 minutes passes a new
// NORMAL one.
SendSummaries()
{
    new address[128], name[MAX_PLAYER_NAME], body[128], queued = 0;

    for (new i = 0; i < MAX_PLAYERS; i++)
    {
        if (!GetPlayerEmail(i, address)) continue;

        GetPlayerName(i, name, sizeof(name));
        format(body, sizeof(body), "Hello %s, you played 12 hours this week.", name);

        new msg = email_new();
        email_add_to(msg, address, name);
        email_set_subject(msg, "Your week");
        email_set_body(msg, body);
        email_set_priority(msg, EMAIL_PRIORITY_LOW);

        if (email_send(msg))
        {
            queued++;
            continue;
        }

        // A refused message is still yours: destroy it.
        if (email_is_message(msg)) email_destroy(msg);
        if (email_errno() == EMAIL_ERROR_QUEUE_FULL) break;   // the rest next time
    }
    return queued;
}

public OnPlayerCommandText(playerid, cmdtext[])
{
    if (!strcmp(cmdtext, "/announce ", true, 10)) return SendAnnouncement(cmdtext[10]);
    if (!strcmp(cmdtext, "/summaries", true))
    {
        new text[64];
        format(text, sizeof(text), "%d summaries queued.", SendSummaries());
        return SendClientMessage(playerid, -1, text);
    }

    // How the queue is doing. EMAIL_EVERY_ACCOUNT adds up every account; pass
    // 0 (or nothing) for just the default one.
    if (!strcmp(cmdtext, "/mailstats", true))
    {
        new queued, sent, failed, text[64];
        email_stats(EMAIL_EVERY_ACCOUNT, queued, sent, failed);
        format(text, sizeof(text), "Mail: %d waiting, %d sent, %d failed", queued, sent, failed);
        return SendClientMessage(playerid, -1, text);
    }
    return 0;
}

// On shutdown the plugin waits up to 10 seconds for queued mail to leave. A
// mailing still going at that point is cut short, so do not start one right
// before a restart. Nothing is kept on disk.

// Stand-in for wherever your gamemode keeps player addresses.
GetPlayerEmail(playerid, dest[], size = sizeof(dest))
{
    #pragma unused playerid, size
    dest[0] = EOS;
    return 0;
}
