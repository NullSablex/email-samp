; Used by 05_registration.pwn. Variables: {name}, {code}, {minutes}.

[subject]
Your confirmation code: {code}

[text]
Hello {name},

Your confirmation code is {code}. Type it in game with /confirm {code}.
It expires in {minutes} minutes.

If you did not register, ignore this message.

[html]
<html>
  <body style="font-family: sans-serif; color: #222">
    <p>Hello <b>{name}</b>,</p>
    <p>Your confirmation code is</p>
    <p style="font-size: 24px; letter-spacing: 4px"><b>{code}</b></p>
    <p>Type it in game with <code>/confirm {code}</code>.
       It expires in {minutes} minutes.</p>
    <p style="color: #777; font-size: 12px">
      If you did not register, ignore this message.
    </p>
  </body>
</html>
