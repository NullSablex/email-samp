; The mail 00_ready_to_send.pwn sends, ready to use.
;
; Edit the wording below; the gamemode does not change. Lines starting with
; ';' are notes and are not sent. {name} and {server} are filled in by the
; gamemode (email_set_var). Save the file and the next mail uses it - no
; restart, no recompile.

[subject]
Welcome to {server}, {name}!

[text]
Hello {name},

Your account on {server} is ready. Log in and come say hello.

If you did not create this account, just ignore this message.

-- The {server} team

[html]
<html>
  <body style="font-family: sans-serif; color: #222">
    <h2>Welcome to {server}, {name}!</h2>
    <p>Your account is ready. Log in and come say hello.</p>
    <p style="color: #777; font-size: 12px">
      If you did not create this account, just ignore this message.
    </p>
  </body>
</html>
