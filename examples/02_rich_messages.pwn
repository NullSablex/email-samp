// 02_rich_messages.pwn - everything email_send_to cannot do: HTML, several
// recipients, attachments, images.
//
// The pattern is always the same: email_new, set what you need, email_send.
// email_send consumes the message - do not use the id afterwards.

#include <a_samp>
#include <email_samp>

public OnGameModeInit()
{
    email_setup();
    return 1;
}

// Text AND HTML: clients that show HTML use it, the rest show the text.
// Sending HTML alone is a reliable way into the spam folder.
SendNewsletter(const address[])
{
    new msg = email_new();
    email_add_to(msg, address);
    email_set_subject(msg, "This week in Los Santos");
    email_set_body(msg, "Double XP this weekend. See you in game!");
    email_set_html(msg, "<h2>This week in Los Santos</h2><p><b>Double XP</b> this weekend.</p>");
    return email_send(msg);
}

// To is visible to everyone, Cc too, Bcc to nobody. For several players use
// Bcc, or each one sees everybody else's address.
SendToStaff()
{
    new msg = email_new();
    email_add_to(msg, "owner@example.com", "Owner");
    email_add_cc(msg, "admin@example.com", "Head admin");
    email_add_bcc(msg, "archive@example.com");

    // A different sender for this message only. Most relays accept only
    // addresses you have verified with them.
    email_set_from(msg, "reports@example.com", "Server reports");

    // Replies go here instead of to the (probably noreply) sender.
    email_set_reply_to(msg, "support@example.com", "Support");

    email_set_subject(msg, "Weekly report");
    email_set_body(msg, "Report attached.");

    // A file from disk. The path is checked now; the file is read when the
    // mail is sent, so a large one never stalls the server.
    email_attach(msg, "scriptfiles/report.txt");

    // Text built in the gamemode, attached without touching the disk.
    email_attach_data(msg, "name,score\nPlayer,100\n", "scores.csv");

    // Any header the plugin has no native for.
    email_add_header(msg, "X-Server", "Los Santos RP");
    return email_send(msg);
}

// An image INSIDE the HTML rather than a download. The HTML refers to it as
// cid:NAME, and NAME is the file name without its extension: logo.png is
// cid:logo. Clients show embedded images; linked ones they often block.
SendWelcome(const address[])
{
    new msg = email_new();
    email_add_to(msg, address);
    email_set_subject(msg, "Welcome to Los Santos RP");
    email_set_body(msg, "Welcome to Los Santos RP!");
    email_set_html(msg, "<img src=\"cid:logo\"><h2>Welcome!</h2>");
    email_embed(msg, "scriptfiles/logo.png");
    return email_send(msg);
}

public OnPlayerCommandText(playerid, cmdtext[])
{
    #pragma unused playerid
    if (!strcmp(cmdtext, "/newsletter", true)) return SendNewsletter("player@example.com");
    if (!strcmp(cmdtext, "/report", true))     return SendToStaff();
    if (!strcmp(cmdtext, "/welcome", true))    return SendWelcome("player@example.com");
    return 0;
}
