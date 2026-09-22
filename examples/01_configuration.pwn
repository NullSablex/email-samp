// 01_configuration.pwn - where the settings come from, and more than one
// account.
//
// Every source takes the same keys: host, user, password, port, encryption,
// from, from_name, rate_limit, retries... (full list in email_samp.inc).
// An unknown key is an error, so a typo never passes silently.

#include <a_samp>
#include <env_samp>     // before email_samp: that is what enables email_setup_env
#include <email_samp>

// The server's accounts, in one place rather than loose globals.
enum E_MAIL_ACCOUNTS
{
    mail_players,       // registration, password resets
    mail_staff          // alerts to the staff inbox, from its own sender
}
new Mail[E_MAIL_ACCOUNTS];

public OnGameModeInit()
{
    // RECOMMENDED: a .env file, read by env_samp. The keys sit under a
    // prefix, next to the rest of the server's settings (see env.example):
    //
    //     smtp.host      = smtp.gmail.com
    //     smtp.user      = bot@gmail.com
    //     smtp.password  = "abcd efgh ijkl mnop"
    //     smtp.from_name = Los Santos RP
    //
    Mail[mail_players] = email_setup_env();

    // A second account is just another prefix: smtp.staff.host, and so on.
    Mail[mail_staff] = email_setup_env("smtp.staff");

    // While working on the gamemode, smtp.dry_run = 1 writes each mail to
    // logs/dry-run/ as an .eml file and sends nothing - no quota spent and no
    // real player mailed by accident.

    // The other sources, if you do not use env_samp:
    //
    //   email_setup();                          SMTP_* environment variables
    //                                           (Docker, systemd), else smtp.ini
    //   email_setup("config/mail.ini");         a file of your choice
    //   email_setup("smtps://user:pass@host");  everything in one URL
    //   email_connect(host, user, pass, "port=465; rate_limit=60");
    //                                           values you have in code
    return 1;
}

// The FIRST account opened is the default: every native that takes an
// account uses it when you pass nothing. Any other account is addressed
// by its id.
SendStaffAlert(const text[])
{
    new msg = email_new(Mail[mail_staff]);
    email_add_to(msg, "staff@example.com");
    email_set_subject(msg, "Server alert");
    email_set_body(msg, text);
    return email_send(msg);
}

public OnPlayerCommandText(playerid, cmdtext[])
{
    if (!strcmp(cmdtext, "/alert ", true, 7))
    {
        SendStaffAlert(cmdtext[7]);
        return 1;
    }

    // Where each account points, without credentials: "host:port MODE".
    if (!strcmp(cmdtext, "/mailstatus", true))
    {
        new status[128];
        email_status(Mail[mail_players], status);
        SendClientMessage(playerid, -1, status);
        return 1;
    }
    return 0;
}
