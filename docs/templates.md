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
email_set_var(msg, "name",  nick);           // text, as written
email_set_var(msg, "slots", "%d", 200);      // a number
email_set_var(msg, "vip",   "%d", true);     // a bool: 1 or 0
email_set_var(msg, "saldo", "%.2f", money);  // decimals, your choice
email_set_var(msg, "line",  "%s (%d)", name, level);
```

!!! warning "The specifiers do not escape anything"
    `%d`, `%s` and `%f` are not a safety measure, and you do not need them to make a value safe. Escaping is automatic and happens later, when the value lands in the `[html]` part. A specifier only tells a variadic call which kind of cell follows — the same reason `format` needs one — and `%.Nf` additionally picks the decimals.

So **text needs no specifier at all**. With nothing after the value it is used exactly as written, which is why a nickname containing `%` needs no escaping either.

| Specifier | Value |
|---|---|
| `%d`, `%i` | an integer, or a bool (`1` / `0`) |
| `%f`, `%.Nf` | a float; six decimals, or `N` of them |
| `%s` | a string, when joining it with something else |
| `%r` | a string, **with the automatic escaping turned off** |
| `%%` | a literal percent sign |

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
