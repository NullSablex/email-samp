; Used by 03_templates.pwn:  email_set_template(msg, "templates/welcome.tpl")
;
; Sections: [subject], [text] and [html], all optional, in any order.
; Everything before the first section is ignored - like these comments.
;
; {name} is replaced with the value of email_set_var(msg, "name", ...). An
; unknown marker is left as it is, so a typo shows up in the mail.
; Write {{ and }} for a literal brace (CSS needs it).

[subject], [text] (aliases: [body], [plain]) and [html].
; Any of them may be omitted, and they may appear in any order. Everything
; before the first section header is ignored, which is what makes this
; comment block legal.
;
; {name} markers are filled from the message's variables (email_set_var).
; An unknown marker is left visible, braces and all - a typo you can see and
; report beats an empty string nobody notices.
;
; Write {{ and }} for literal braces, which is what CSS in the HTML part needs.

[subject]
Welcome to {server}, {name}!

[text]
Hello {name},

Your account on {server} is ready. The server has {slots} slots and we are
glad to have you. Your starting bonus is ${bonus}.

If you did not create this account, you can ignore this message.

-- The {server} team

[html]
<html>
  <body style="font-family: sans-serif; color: #222">
    <h2>Welcome to {server}, {name}!</h2>
    <p>Your account is ready. The server has <b>{slots}</b> slots
       and we are glad to have you. Your starting bonus is <b>${bonus}</b>.</p>
    <p style="color: #777; font-size: 12px">
      If you did not create this account, you can ignore this message.
    </p>
  </body>
</html>
