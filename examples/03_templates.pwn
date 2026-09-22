// 03_templates.pwn - the wording in a file, the values from the gamemode.
//
// Two file formats, both fine:
//
//   .tpl    [subject], [text] and [html] sections in one file, so the same
//           mail carries a plain-text version for clients (and spam filters)
//           that want one. See templates/welcome.tpl.
//   .html   a plain HTML file, like the ones you would send from PHP: no
//           sections, and its <title> becomes the subject.
//           See templates/welcome.html.
//
// Both use {marker} for the values. Edit the file and the next mail uses it -
// no restart, no recompile.

#include <a_samp>
#include <email_samp>

public OnGameModeInit()
{
    email_setup();
    return 1;
}

SendWelcome(playerid, const address[])
{
    new name[MAX_PLAYER_NAME];
    GetPlayerName(playerid, name, sizeof(name));

    new msg = email_new();
    email_add_to(msg, address, name);

    // The file is read here, so a wrong path fails on this line. It is
    // filled in when the mail is sent, so the order of these calls is free.
    email_set_template(msg, "templates/welcome.tpl");
    email_set_var(msg, "name", name);
    email_set_var(msg, "server", "Los Santos RP");
    // Text goes as written. A number needs "%d" only because Pawn cannot
    // pass one where a string is expected - it formats nothing, the value
    // becomes text either way. "%.2f" is different: it decides the decimals.
    email_set_var(msg, "slots", "%d", GetMaxPlayers());
    email_set_var(msg, "bonus", "%.2f", 2500.0);
    return email_send(msg);
}

SendPasswordReset(playerid, const address[], const code[])
{
    new name[MAX_PLAYER_NAME];
    GetPlayerName(playerid, name, sizeof(name));

    new msg = email_new();
    email_add_to(msg, address, name);
    email_set_template(msg, "templates/password_reset.tpl");
    email_set_var(msg, "name", name);
    email_set_var(msg, "code", code);
    email_set_var(msg, "minutes", "%d", 15);

    // A value set on the message wins over the template's section.
    email_set_subject(msg, "Your reset code");
    return email_send(msg);
}

// Same call, pointing at an HTML file instead.
SendWelcomeHtml(playerid, const address[])
{
    new name[MAX_PLAYER_NAME];
    GetPlayerName(playerid, name, sizeof(name));

    new msg = email_new();
    email_add_to(msg, address, name);
    email_set_template(msg, "templates/welcome.html");
    email_set_var(msg, "name", name);
    email_set_var(msg, "server", "Los Santos RP");
    return email_send(msg);
}

// Values are escaped for you where it matters. A nickname with <script> in it
// arrives as text in the [html] part - you do not add %s or anything else to
// make that happen, and %s would not do it anyway.
//
// %r is the one way to turn that off, for markup YOU built. Never for
// anything a player typed.
SendReport(const address[])
{
    new rows[512];
    format(rows, sizeof(rows), "<tr><td>%s</td><td>%d</td></tr>", "Total", 42);

    new msg = email_new();
    email_add_to(msg, address);
    email_set_template(msg, "templates/welcome.html");
    email_set_var(msg, "name", "Admin");        // escaped, as everything is
    email_set_var(msg, "server", "%r", rows);   // markup on purpose
    return email_send(msg);
}

// Editing a .tpl or .html while the server runs is enough: the next mail uses
// it. This is for when that is not detected - an editor or a deploy script
// that puts the old timestamp back.
ReloadMailTemplates(playerid)
{
    new text[64];
    format(text, sizeof(text), "%d template(s) will be read again.", email_force_reload_templates());
    return SendClientMessage(playerid, -1, text);
}

// A file with nothing to fill in needs no message at all: one line sends it,
// subject included.
SendMaintenanceNotice(const address[])
{
    return email_send_file(address, "templates/maintenance.html");
}

public OnPlayerCommandText(playerid, cmdtext[])
{
    if (!strcmp(cmdtext, "/welcome", true)) return SendWelcome(playerid, "player@example.com");
    if (!strcmp(cmdtext, "/reset", true))   return SendPasswordReset(playerid, "player@example.com", "AB12CD");
    if (!strcmp(cmdtext, "/welcomehtml", true)) return SendWelcomeHtml(playerid, "player@example.com");
    if (!strcmp(cmdtext, "/maintenance", true)) return SendMaintenanceNotice("player@example.com");
    if (!strcmp(cmdtext, "/report", true))      return SendReport("player@example.com");
    if (!strcmp(cmdtext, "/reloadmail", true))  return ReloadMailTemplates(playerid);
    return 0;
}

// Values are pasted as-is and never read as markers, so a player called
// "{code}" gets that literal text, not someone's reset code. An unknown
// marker stays visible in the mail, braces and all, so a typo is obvious.
