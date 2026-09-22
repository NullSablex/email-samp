# Templates

A template is a **file** with the wording in it, and `{markers}` where the gamemode's values go. Edit the file, save, and the next mail uses it: no restart, no recompile. There is no handle to create and nothing to destroy — the file already has a name.

```pawn
new msg = email_new();
email_add_to(msg, address, name);
email_set_template(msg, "mails/welcome.tpl");
email_set_var(msg, "name", name);
email_send(msg);
```

## Two formats

### `.tpl` — subject, text and HTML in one file

```ini
; Everything before the first section is ignored, so this comment is fine.

[subject]
Welcome to {server}, {name}!

[text]
Hello {name},

Your account on {server} is ready.

[html]
<p>Hello <b>{name}</b>, your account on {server} is ready.</p>
```

Sections are optional and may appear in any order; `[body]` and `[plain]` are aliases of `[text]`. Write `{{` and `}}` for a literal brace, which is what CSS in the HTML part needs.

This is the format worth using when the mail matters: it carries the plain-text version that clients without HTML — and spam filters — expect.

### `.html` — a plain HTML file

The kind you already have, exported from a designer or copied from a PHP project. No sections, nothing to learn:

```html
<html>
  <head><title>Welcome to {server}, {name}!</title></head>
  <body><p>Your account is ready.</p></body>
</html>
```

The `<title>` becomes the subject. Any other file with no sections is used as the plain-text body.

## Values

One native takes every kind of value:

```pawn
email_set_var(msg, "name",  nick);            // text, as written
email_set_var(msg, "slots", "%d", 200);       // a number
email_set_var(msg, "saldo", "%.2f", money);   // and its decimals
```

A template has no arithmetic and no conditions, so **every value becomes text**, whatever it started as. That is the whole model, and it is why there is so little to learn here:

- **Text** needs no specifier. Pass it as the value and it is used exactly as written — which is also why a nickname containing `%` needs no escaping.
- **A number** needs `"%d"` for one reason: Pawn cannot pass a number where a string is expected. It formats nothing; the result is the same text either way. A bool goes through `"%d"` too, as `1` or `0`.

Only two specifiers actually decide anything:

| Specifier | What it decides |
|---|---|
| `%.Nf` | how many decimals a float keeps (`%f` alone means six) |
| `%r` | turns the automatic HTML escaping **off**, for markup you built |

!!! warning "The rest decide nothing, and protect nothing"
    `%d` and `%s` are not a safety measure, and you never need them to make a value safe: escaping is automatic and happens when the value lands in the `[html]` part. Reaching for a specifier because the value "looks dangerous" is a misunderstanding — the one that changes safety is `%r`, and it changes it in the *unsafe* direction.

### Escaping, and the one way out

A value substituted into the `[html]` part is HTML-escaped, so a nickname containing `<script>` arrives as text. The markup comes from the template file, which the server owns; the values are what a player may have typed.

`%r` is the only way to put markup of your own in:

```pawn
email_set_var(msg, "rows", "%r", built_html);   // you built it
email_set_var(msg, "nick", nick);               // a player typed it
```

Never `%r` for anything a player typed: that is exactly the escaping you would be turning off.

### What a marker cannot do

- A value is **never re-scanned**. A value containing `{another_key}` stays literal, so a nickname cannot be used to read the value of another variable.
- An **unknown marker stays visible** in the mail, braces and all. A typo you can see beats an empty string nobody notices.
- The **rendered subject is re-checked** for line breaks, since a value could carry one.

## Order does not matter

The file is **read** when you call `email_set_template`, so a wrong path fails on that line. It is **rendered** at send time, so `email_set_template` and `email_set_var` can be called in any order.

A template fills in only what the message left empty, so `email_set_subject` after `email_set_template` wins — which is the way round anyone expects.

## Cache and reloading

Templates are cached per file, so a mailing does not re-read the same file a thousand times. The cache notices a change in the file's size or modification time, so editing the wording with the server running is enough: the next mail uses it.

`email_force_reload_templates()` covers what that misses — a deploy that copies with the original timestamps (`rsync -a`, `scp -p`), or a network share with a coarse clock:

```pawn
printf("[email] %d template(s) will be read again", email_force_reload_templates());
```

Messages already queued keep the version they were built with.
