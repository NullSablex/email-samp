; Used by 03_templates.pwn. Variables: {name}, {code}, {minutes}.

[subject]
Password reset for {name}

[text]
Hello {name},

Your reset code is {code}. It expires in {minutes} minutes.

If you did not ask for a password reset, ignore this message and nothing will
change.

[html]
<html>
  <body style="font-family: sans-serif; color: #222">
    <p>Hello <b>{name}</b>,</p>
    <p>Your reset code is
       <code style="font-size: 18px; letter-spacing: 2px">{code}</code>.<br>
       It expires in {minutes} minutes.</p>
    <p style="color: #777; font-size: 12px">
      If you did not ask for a password reset, ignore this message and nothing
      will change.
    </p>
  </body>
</html>
