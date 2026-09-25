/*
 * email_samp v0.1.0 - SMTP mail plugin for SA-MP and Open Multiplayer.
 * Author:     NullSablex
 * Repository: https://github.com/NullSablex/email-samp
 * License:    GPL-3.0
 *
 * GENERATED from email_samp.inc.in by build.rs - do not edit by hand.
 *
 * Nothing here blocks: every native returns at once and the mail leaves on a
 * worker thread, reporting back through a callback on a later tick.
 *
 * The whole of the common case:
 *
 *     email_setup();
 *     email_send_to("player@example.com", "Welcome", "Your account is ready.");
 *
 * Contents
 *   1. Callback shorthand
 *   2. Configuration keys
 *   3. Constants
 *   4. Callbacks
 *   5. Setup and accounts
 *   6. Errors
 *   7. Sending in one line
 *   8. Building a message
 *   9. Utility
 *  10. env_samp
 */

#if defined _email_samp_included
    #endinput
#endif
#define _email_samp_included

#pragma library email_samp

#define EMAIL_SAMP_VERSION "0.1.0"


// ---------------------------------------------------------------------------
// 1. Callback shorthand
// ---------------------------------------------------------------------------

// Declares a callback in one line:
//
//     Email::OnMailSent(success, playerid) { ... }
//
// is the same as the usual pair, and both work:
//
//     forward OnMailSent(success, playerid);
//     public  OnMailSent(success, playerid) { ... }
#define Email::%0(%1) forward %0(%1); public %0(%1)


// ---------------------------------------------------------------------------
// 2. Configuration keys
// ---------------------------------------------------------------------------

// Read by email_setup(), and the same names in every source: a .env through
// env_samp, the process environment, an SMTP URL, smtp.ini, or the settings
// string email_connect takes. Keys are case-insensitive and an optional SMTP_
// prefix is dropped, so SMTP_HOST, smtp_host and host are one key. An
// UNRECOGNISED key is an error, not a silent no-op.
//
//   url          smtps://user:pass@host:465 - carries everything at once
//   host         relay hostname                      (required, unless url)
//   user         SMTP username                       (alias: username)
//   password     SMTP password                       (alias: pass)
//   port         default follows encryption
//   encryption   none | starttls | tls               (default: starttls)
//   from         From: address                       (default: the username)
//   from_name    display name shown to recipients
//   helo         EHLO name                           (default: this hostname)
//   timeout      seconds                             (default: 30)
//   pool_size    pooled SMTP sessions                (default: 4)
//   tls_ca       PEM root cert, for a private CA
//   tls_verify   1/0, true/false, yes/no, on/off     (default: 1)
//   retries      extra attempts after a TEMPORARY failure (default: 2). An
//                unreachable relay pauses the whole account instead, so an
//                outage does not burn every queued message's retries.
//   rate_limit   messages per minute, evenly spaced; 0 = unlimited
//                (default: 0). Other accounts are never held back by it.
//   queue_limit  messages waiting at once; past it a send fails with
//                EMAIL_ERROR_QUEUE_FULL              (default: 1000)
//   charset      how Pawn's 8-bit strings are read. Any encoding name works -
//                windows-1252 (default, what SA-MP uses), windows-1251,
//                windows-1250/1253/1254/1256/1257, iso-8859-2, utf-8 for a
//                gamemode that already stores UTF-8. Reading it wrong is what
//                turns "Joao" with a tilde into "Jo?o" in the mail.
//   allow_plaintext_auth
//                1 allows sending the password over an unencrypted connection
//                to a host that is NOT this machine. Without it such a setup
//                is refused, because anyone on the path reads the password.
//                A relay on localhost never needs it.   (default: 0)
//   dry_run      1 writes each message to logs/dry-run/ as an .eml file and
//                sends nothing - for working on a gamemode without spending
//                quota or mailing real players       (default: 0)


// ---------------------------------------------------------------------------
// 3. Constants
// ---------------------------------------------------------------------------

// Send queue priorities (email_set_priority).
//
// Each level is a head start, not a separate queue: a LOW message that has
// waited 10 minutes outranks a NORMAL one queued just now, so nothing starves.
// LOW may fill only three quarters of queue_limit, so a mailing can never lock
// out a password reset; HIGH is never refused by the limit.
enum {
    // Newsletters, announcements - anything nobody is waiting on.
    EMAIL_PRIORITY_LOW = -1,
    // The default.
    EMAIL_PRIORITY_NORMAL = 0,
    // A player is waiting for it: a confirmation code, a password reset.
    EMAIL_PRIORITY_HIGH = 1,
}

// Error codes (email_errno)
enum {
    EMAIL_ERROR_NONE = 0,
    EMAIL_ERROR_INVALID_ACCOUNT,
    EMAIL_ERROR_INVALID_MESSAGE,
    EMAIL_ERROR_TEMPLATE_FAILED,
    EMAIL_ERROR_INVALID_ADDRESS,
    EMAIL_ERROR_CONNECTION_FAILED,
    EMAIL_ERROR_AUTH_FAILED,
    EMAIL_ERROR_SEND_FAILED,
    // The message could not be assembled: no recipient, an embedded image
    // with no HTML body, or a per-message limit (100 recipients, 50 custom
    // headers, 5000 open drafts).
    EMAIL_ERROR_BUILD_FAILED,
    EMAIL_ERROR_ATTACHMENT_FAILED,
    // A line break in a value that becomes a header. It would start a new
    // header - a hidden Bcc, say - so it is refused. Treat it as an attack,
    // not a typo.
    EMAIL_ERROR_HEADER_INJECTION,
    EMAIL_ERROR_CONFIG_FAILED,
    // The account already has queue_limit messages waiting. The message was
    // NOT consumed: send it again later, or email_destroy it.
    EMAIL_ERROR_QUEUE_FULL,
}

// email_stats: every account added up, instead of one.
#define EMAIL_EVERY_ACCOUNT (-1)

// Limits (email_limit). The plugin enforces these; ask for them instead of
// writing the numbers into the gamemode, so a future change cannot go unnoticed.
enum {
    // Recipients per message, across To + Cc + Bcc.
    EMAIL_LIMIT_RECIPIENTS = 0,
    // Custom headers per message (email_add_header).
    EMAIL_LIMIT_HEADERS,
    // Total attachment bytes per message, before encoding.
    EMAIL_LIMIT_ATTACHMENT_BYTES,
    // Drafts open at once, across every script.
    EMAIL_LIMIT_DRAFTS,
    // Longest value for one header (subject, display name, ...).
    EMAIL_LIMIT_HEADER_LEN,
}

// Log levels (email_log)
enum {
    EMAIL_LOG_NONE = 0,
    EMAIL_LOG_ERROR,
    EMAIL_LOG_WARNING,
    EMAIL_LOG_INFO,
    EMAIL_LOG_ALL,
}


// ---------------------------------------------------------------------------
// 4. Callbacks
// ---------------------------------------------------------------------------

/**
 * Fired on every loaded script when a message leaves.
 *
 * The counterpart of OnEmailError: it lets a filterscript keep the mail log
 * without the gamemode passing a callback to each send.
 *
 * @param account    the account the message was sent on
 * @param recipient  the first recipient
 * @param callback   the callback email_send was given, or "" if none
 */
forward OnEmailSent(account, const recipient[], const callback[]);

/**
 * Fired on every loaded script when a send fails for good - after the retries
 * a temporary failure gets, never once per attempt.
 *
 * Your send callback also runs, with success = 0. Use the callback to tell the
 * player, and this to log the reason.
 *
 * @param account    the account the message belonged to, or 0
 * @param recipient  the first recipient, so the line is actionable
 * @param callback   the callback email_send was given, or "" if none
 * @param error      the relay's reply, or the plugin's explanation
 * @param errorid    an EMAIL_ERROR_* value
 */
forward OnEmailError(account, const recipient[], const callback[], const error[], errorid);


// ---------------------------------------------------------------------------
// 5. Setup and accounts
// ---------------------------------------------------------------------------

/**
 * Reads the configuration and opens an account, in one line.
 *
 * The account becomes the DEFAULT, which is what account = 0 means everywhere
 * else. The FIRST account opened stays the default, so a filterscript opening
 * its own does not redirect the gamemode's mail.
 *
 * Nothing is dialled here: the first TCP + TLS + AUTH happens on the first
 * send. Use email_test to check the credentials at startup.
 *
 * @param source  "" to auto-discover (process environment, then smtp.ini); an
 *                SMTP URL such as smtps://user:pass@host; or a file path
 * @return account id (>= 1), or 0 on failure - email_errno(0) says why
 */
native email_setup(const source[] = "");

/**
 * Opens an account from values supplied in code.
 *
 * For a second account or credentials read at runtime; prefer email_setup for
 * the first one. The credentials are separate arguments so a password needs no
 * escaping; everything else goes in `settings`, using the keys listed at the
 * top of this file, separated by `;`.
 *
 * @param host      relay hostname
 * @param user      SMTP username, or "" for a relay that takes no login
 * @param password  SMTP password, or ""
 * @param settings  e.g. "port=465; from=noreply@example.com; rate_limit=30"
 * @return account id (>= 1), or 0 on failure - email_errno(0) says why
 */
native email_connect(const host[], const user[], const password[], const settings[] = "");

/**
 * Closes an account, dropping its pooled sessions and every unsent draft bound
 * to it. Messages already handed to email_send still complete.
 *
 * @param account  account id, or 0 for the default account
 * @return true on success, false if the id is unknown
 */
native bool:email_close(account = 0);

/**
 * Whether an account handle is still open. With no argument, whether mail is
 * configured at all.
 *
 * @param account  account id, or 0 for the default account
 * @return true if the account exists
 */
native bool:email_is_account(account = 0);

/**
 * Writes "host:port MODE" into dest - e.g. "smtp.gmail.com:587 STARTTLS".
 *
 * Safe to show in a debug command: it contains no credentials.
 *
 * @param account   account id, or 0 for the default account
 * @param dest      destination buffer
 * @param dest_len  size of dest
 * @return true on success, false if the account is unknown
 */
native bool:email_status(account, dest[], dest_len = sizeof(dest));

/**
 * How the mail queue is doing, and what it has done, since the account was
 * opened:
 *
 *     new waiting, sent, failed;
 *     email_stats(EMAIL_EVERY_ACCOUNT, waiting, sent, failed);
 *     printf("%d waiting, %d sent, %d failed", waiting, sent, failed);
 *
 * @param account  account id, 0 for the default account, or
 *                 EMAIL_EVERY_ACCOUNT for every account added up
 * @param queued   still to go out: waiting, being retried, or in flight
 * @param sent     messages handed to the relay (email_test is not one)
 * @param failed   failed for good, after any retries
 * @return true on success, false if the account is unknown
 */
native bool:email_stats(account = 0, &queued = 0, &sent = 0, &failed = 0);

/**
 * Opens a session, authenticates and closes it, to prove the configuration
 * works. Threaded, like every send.
 *
 * The callback receives the result FIRST, then the extras described by format:
 * email_test(0, "OnTested", "d", 1) calls OnTested(success, 1).
 *
 * @param account   account id, or 0 for the default account
 * @param callback  public to call when the test finishes, or "" for none
 * @param format    specifiers for the extra arguments: d/i int, f float, s string
 * @return true if the test was queued, false on an unknown account
 */
native bool:email_test(account = 0, const callback[] = "", const format[] = "", {Float,_}:...);


// ---------------------------------------------------------------------------
// 6. Errors
// ---------------------------------------------------------------------------

/**
 * Last error code for an account.
 *
 * @param account  account id, or 0 for the default account. With no account
 *                 open, 0 is the global slot, where failures from before one
 *                 existed land (a bad config file, a malformed sender)
 * @return an EMAIL_ERROR_* value, or 0 for none
 */
native email_errno(account = 0);

/**
 * Writes the last error message into dest, usually the relay's own reply.
 *
 * Read it right after a native returned false. For a send failure take the
 * reason from OnEmailError instead: by the time your callback runs, another
 * send may have overwritten the slot.
 *
 * @param account   account id, or 0 for the default account (the global slot
 *                  when none is open)
 * @param dest      destination buffer
 * @param dest_len  size of dest
 * @return true on success
 */
native bool:email_error(account, dest[], dest_len = sizeof(dest));


// ---------------------------------------------------------------------------
// 7. Sending in one line
// ---------------------------------------------------------------------------

/**
 * Sends a plain-text mail to one address on the default account.
 *
 * The callback receives the result FIRST, then the extras described by format:
 * email_send_to(addr, s, b, "OnMailSent", "d", playerid) calls
 * OnMailSent(success, playerid).
 *
 * For HTML, attachments, templates or extra recipients, build the message
 * instead (section 8).
 *
 * @param to        recipient address
 * @param subject   the subject
 * @param body      the plain-text body
 * @param callback  public to call when the send finishes, or "" for none
 * @param format    specifiers for the extra arguments: d/i int, f float, s string
 * @return true if the message was queued, false if mail is not configured or
 *         an argument was rejected
 */
native bool:email_send_to(const to[], const subject[], const body[], const callback[] = "", const format[] = "", {Float,_}:...);

/**
 * Sends a template file to one address; the file carries the subject and the
 * wording, so the gamemode carries neither:
 *
 *     email_send_file(address, "mails/notice.html");
 *
 * For a file with {markers} in it, build the message instead: the values are
 * what this call has no room for.
 *
 * @param to        recipient address
 * @param path      path to the .tpl or .html file
 * @param callback  public to call when the send finishes, or "" for none
 * @param format    specifiers for the extra arguments: d/i int, f float, s string
 * @return true if the message was queued, false if mail is not configured, the
 *         file is unreadable, or the address was rejected
 */
native bool:email_send_file(const to[], const path[], const callback[] = "", const format[] = "", {Float,_}:...);


// ---------------------------------------------------------------------------
// 8. Building a message
// ---------------------------------------------------------------------------
//
// new msg = email_new();
// email_add_to(msg, address, name);
// email_set_subject(msg, "Welcome");
// email_set_body(msg, "Your account is ready.");
// email_send(msg);

/**
 * Creates an empty message draft.
 *
 * The draft belongs to the calling script, so a gamemode restart reclaims it
 * even if the script never calls email_destroy. Up to 5000 may be open at
 * once: a draft that is never sent must be destroyed.
 *
 * @param account  account id, or 0 for the default account
 * @return message id (>= 1), or 0 if the account is unknown
 */
native email_new(account = 0);

/**
 * Discards a draft that will not be sent. Unnecessary after a successful
 * email_send, which consumes the handle.
 *
 * @param message  message id
 * @return true on success, false if the id is unknown
 */
native bool:email_destroy(message);

/**
 * Whether a handle is still a live draft. False after a successful email_send.
 *
 * @param message  message id
 * @return true if the draft exists
 */
native bool:email_is_message(message);

/**
 * Adds a visible recipient.
 *
 * The address is parsed now, so a typo fails on this line rather than inside a
 * send callback minutes later. A message may not exceed 100 recipients across
 * To, Cc and Bcc.
 *
 * @param message  message id
 * @param address  recipient address
 * @param name     display name, or "" for a bare address
 * @return true on success, false on a bad handle, a malformed address, or a
 *         line break in either argument (EMAIL_ERROR_HEADER_INJECTION)
 */
native bool:email_add_to(message, const address[], const name[] = "");

/**
 * Adds a carbon-copy recipient, visible to everyone else on the message.
 *
 * @param message  message id
 * @param address  recipient address
 * @param name     display name, or ""
 * @return true on success
 */
native bool:email_add_cc(message, const address[], const name[] = "");

/**
 * Adds a blind carbon-copy recipient, hidden from the other recipients.
 *
 * The right field for mailing several players at once: a Cc list would publish
 * every player's address to every other player.
 *
 * @param message  message id
 * @param address  recipient address
 * @param name     display name, or ""
 * @return true on success
 */
native bool:email_add_bcc(message, const address[], const name[] = "");

/**
 * Overrides the account's From: identity for this message only.
 *
 * Most relays refuse a From: they do not consider you authorised to use, so
 * this is for a second verified sender, not for spoofing.
 *
 * @param message  message id
 * @param address  sender address
 * @param name     display name, or "" for a bare address
 * @return true on success, false on a bad handle or a rejected address
 */
native bool:email_set_from(message, const address[], const name[] = "");

/**
 * Sets where replies should go. Useful when mail is sent from a noreply@
 * account but answers should reach an inbox someone reads.
 *
 * @param message  message id
 * @param address  reply-to address
 * @param name     display name, or ""
 * @return true on success
 */
native bool:email_set_reply_to(message, const address[], const name[] = "");

/**
 * Sets the subject line.
 *
 * Non-ASCII is fine and encoded per RFC 2047 at send time. A line break is
 * NOT: it would start a new header, so it is rejected.
 *
 * @param message  message id
 * @param subject  the subject
 * @return true on success, false on a line break (EMAIL_ERROR_HEADER_INJECTION)
 */
native bool:email_set_subject(message, const subject[]);

/**
 * Sets the plain-text body.
 *
 * Line breaks are legal here and are encoded, not interpreted: the body cannot
 * reach the header block.
 *
 * @param message  message id
 * @param body     the text
 * @return true on success, false if the handle is unknown
 */
native bool:email_set_body(message, const body[]);

/**
 * Sets the HTML body.
 *
 * Setting both this and email_set_body produces a multipart/alternative
 * message: clients that render HTML use it, the rest fall back to the text.
 * Sending HTML alone scores badly with spam filters, so write both.
 *
 * The markup is sent as written. Put player text in through a template
 * (email_set_template) instead: its [html] section escapes every value.
 *
 * @param message  message id
 * @param html     the markup
 * @return true on success
 */
native bool:email_set_html(message, const html[]);

/**
 * Adds a header the plugin has no native of its own for - X-Priority,
 * List-Unsubscribe, a tracking id.
 *
 * Up to 50 per message.
 *
 * @param message  message id
 * @param name     header name: printable ASCII, no colon
 * @param value    header value, with no line breaks
 * @return true on success, false if either half is rejected
 */
native bool:email_add_header(message, const name[], const value[]);

/**
 * Sets where the message stands in the send queue. Recipients never see it;
 * it is not the X-Priority header (use email_add_header for that).
 *
 * @param message   message id
 * @param priority  an EMAIL_PRIORITY_* value (default EMAIL_PRIORITY_NORMAL)
 * @return true on success, false on a bad handle or an unknown priority
 */
native bool:email_set_priority(message, priority);

/**
 * Attaches a file from disk.
 *
 * The path, the size and the MIME type are checked now, so a mistake fails on
 * this line; the contents are read in the worker thread at send time, so a
 * large file never blocks the server. Attachments may total 25 MB per message.
 *
 * The file must be inside the server folder (`..` is fine if it stays inside)
 * and the path may not go through a symlink - so a path built from player
 * input can never mail out server.cfg.
 *
 * @param message   message id
 * @param path      path to the file
 * @param filename  name the recipient sees, or "" for the file's own name
 * @param mime      MIME type, or "" to guess from the extension
 * @return true on success, false if the file is unusable
 */
native bool:email_attach(message, const path[], const filename[] = "", const mime[] = "");

/**
 * Attaches text generated in the gamemode - a CSV export, a JSON dump -
 * without writing it to disk first.
 *
 * Text only: a Pawn string ends at the first NUL, so binary data cannot
 * survive the trip. Use email_attach for that.
 *
 * @param message   message id
 * @param data      the content
 * @param filename  name the recipient sees
 * @param mime      MIME type, or "" to guess from the filename
 * @return true on success
 */
native bool:email_attach_data(message, const data[], const filename[], const mime[] = "");

/**
 * Embeds an image for the HTML body to show inline instead of as a download:
 *
 *     email_embed(msg, "scriptfiles/logo.png");
 *     email_set_html(msg, "<img src=\"cid:logo\"> Welcome!");
 *
 * Needs an HTML body; the send fails with EMAIL_ERROR_BUILD_FAILED without
 * one. The path follows the same rules as email_attach.
 *
 * @param message  message id
 * @param path     path to the image
 * @param cid      name the HTML refers to as cid:NAME, or "" for the file name
 *                 without its extension (logo.png -> logo)
 * @return true on success, false if the file is unusable
 */
native bool:email_embed(message, const path[], const cid[] = "");

/**
 * Points the message at a template file - the wording, in a file you can edit
 * without recompiling.
 *
 * Two formats, both filled in with email_set_var:
 *   .tpl    [subject], [text] and [html] sections in one file. Write {{ and }}
 *           for literal braces, which is what CSS in the HTML part needs.
 *   .html   a plain HTML file, no sections; its <title> becomes the subject.
 * Any other file with no sections is used as the plain-text body.
 *
 * The file is READ now, so a wrong path fails on this line. It is RENDERED at
 * send time, so the order of this call and the email_set_var calls does not
 * matter. The template fills in only the parts the message left empty, so
 * email_set_subject still overrides its subject.
 *
 * Templates are cached and reloaded when the file changes on disk. The path
 * follows the same rules as email_attach.
 *
 * @param message  message id
 * @param path     path to the template file
 * @return true on success, false if the file cannot be read
 */
native bool:email_set_template(message, const path[]);

/**
 * Sets {key} for this message:
 *
 *     email_set_var(msg, "name",  nick);            // text, as written
 *     email_set_var(msg, "slots", "%d", 200);       // a number
 *     email_set_var(msg, "saldo", "%.2f", money);   // and its decimals
 *
 * A template has no arithmetic and no conditions: every value becomes text,
 * whatever it started as. So there is nothing to choose for text - pass it as
 * the value and it is used exactly as written, which is also why a nickname
 * containing '%' needs no escaping.
 *
 * A number needs "%d" for one reason only: Pawn cannot pass a number where a
 * string is expected. It formats nothing - the result is the same text either
 * way. A bool goes through "%d" too, as 1 or 0.
 *
 * Only two specifiers actually decide something:
 *
 *   %.Nf   how many decimals a float keeps ("%f" alone means six)
 *   %r     turns the automatic HTML escaping OFF, for markup you built
 *
 * THE OTHERS DO NOT ESCAPE OR PROTECT ANYTHING, and you do not need them to
 * make a value safe: escaping is automatic and happens when the value lands
 * in the template's [html] section.
 *
 * %r is the only way to put markup of your own in there:
 *
 *     email_set_var(msg, "rows", "%r", built_html);   // you built it
 *     email_set_var(msg, "nick", nick);               // a player typed it
 *
 * Never %r for anything a player typed: that is exactly the escaping you
 * would be turning off.
 *
 * The value is substituted verbatim and never re-scanned, so a value
 * containing {another_key} stays literal - a nickname cannot be used to read
 * the value of another variable. An unknown marker is left visible in the
 * mail, braces and all: a typo you can see beats an empty string nobody
 * notices.
 *
 * @param message  message id
 * @param key      marker name, without the braces
 * @param value    the text, or a format for the arguments that follow
 * @return true on success, false on a bad handle or an empty key
 */
native bool:email_set_var(message, const key[], const value[], {Float,_}:...);

/**
 * Hands the draft to a worker thread and returns immediately.
 *
 * CONSUMES the message handle on success: after that the id is dead, so do not
 * call email_destroy on it and do not send it twice. That is what stops a
 * draft being changed while the worker is serialising it.
 *
 * The callback receives the result FIRST, then the extras described by format:
 * email_send(msg, "OnMailSent", "d", playerid) calls
 * OnMailSent(success, playerid).
 *
 * A true return only means the message was queued. Delivery is reported
 * through the callback and OnEmailError, and a temporary failure is retried
 * first, so the callback runs once with the final outcome.
 *
 * On a false return the handle is NOT consumed: with EMAIL_ERROR_QUEUE_FULL,
 * send it again later or email_destroy it.
 *
 * @param message   message id
 * @param callback  public to call when the send finishes, or "" for none
 * @param format    specifiers for the extra arguments: d/i int, f float, s string
 * @return true if the message was queued, false on a bad handle, a closed
 *         account or a full queue
 */
native bool:email_send(message, const callback[] = "", const format[] = "", {Float,_}:...);


// ---------------------------------------------------------------------------
// 9. Utility
// ---------------------------------------------------------------------------

/**
 * Whether a string is a valid mailbox, using the same parser the send path
 * uses.
 *
 * Worth calling at registration: a typo caught while the player is still on
 * the server can be corrected, whereas one caught at send time is a silent
 * bounce hours later.
 *
 * @param address  the address to check
 * @return true if the address is valid
 */
native bool:email_is_valid_address(const address[]);

/**
 * Forces every cached template to be read from disk again.
 *
 * Templates reload on their own when a file's size or mtime changes, so
 * editing the wording and saving is normally enough. This is the force: for
 * what that misses, such as a deploy that copies with the original timestamps
 * (rsync -a, scp -p) or a network share with a coarse clock.
 *
 * Messages already queued keep the version they were built with.
 *
 * @return how many cached templates were dropped
 */
native email_force_reload_templates();

/**
 * A limit the plugin enforces, so the gamemode can check before it builds -
 * splitting a mailing into batches of email_limit(EMAIL_LIMIT_RECIPIENTS),
 * say, instead of hardcoding 100.
 *
 * @param limit  an EMAIL_LIMIT_* value
 * @return the limit, or 0 if the constant is unknown
 */
native email_limit(limit);

/**
 * Sets the minimum log level at runtime, for both the console and
 * logs/email.log.
 *
 * @param level  an EMAIL_LOG_* value
 * @return true
 */
native bool:email_log(level);


// ---------------------------------------------------------------------------
// 10. env_samp
// ---------------------------------------------------------------------------
//
// Compiled only when <env_samp> was included BEFORE this file.

#if defined _env_samp_included
/**
 * Opens an account from .env keys read through env_samp.
 *
 * The keys are the ones in section 2, under a prefix:
 *
 *     smtp.host      = smtp.gmail.com
 *     smtp.user      = bot@example.com
 *     smtp.password  = "abcd efgh ijkl mnop"
 *     smtp.from_name = Los Santos RP
 *
 * or a single smtp.url. A second account is just another prefix:
 * email_setup_env("smtp.alerts") reads smtp.alerts.host and so on.
 *
 * @param prefix  key prefix, without the trailing dot
 * @return account id (>= 1), or 0 on failure - email_errno(0) says why
 */
stock email_setup_env(const prefix[] = "smtp")
{
    static const keys[][] = {
        "port", "encryption", "from", "from_name", "helo", "timeout",
        "pool_size", "tls_ca", "tls_verify", "retries", "rate_limit",
        "queue_limit", "dry_run"
    };
    new key[64], value[256];

    format(key, sizeof(key), "%s.url", prefix);
    if (Env(key, value) && value[0] != EOS)
    {
        return email_setup(value);
    }

    new host[128], user[128], password[128], settings[1024];
    format(key, sizeof(key), "%s.host", prefix);
    Env(key, host);
    format(key, sizeof(key), "%s.user", prefix);
    Env(key, user);
    format(key, sizeof(key), "%s.password", prefix);
    Env(key, password);

    for (new i = 0; i < sizeof(keys); i++)
    {
        format(key, sizeof(key), "%s.%s", prefix, keys[i]);
        if (!Env(key, value) || value[0] == EOS)
        {
            continue;
        }

        // key="value"; with " and \ escaped, so no value can end the setting
        // early or start another one.
        strcat(settings, keys[i]);
        strcat(settings, "=\"");
        new len = strlen(settings);
        for (new c = 0; value[c] != EOS && len < sizeof(settings) - 4; c++)
        {
            if (value[c] == '"' || value[c] == '\\')
            {
                settings[len++] = '\\';
            }
            settings[len++] = value[c];
        }
        settings[len] = EOS;
        strcat(settings, "\";");
    }

    return email_connect(host, user, password, settings);
}
#endif
