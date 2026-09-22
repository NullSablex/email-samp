//! Message templates with `{placeholder}` substitution.
//!
//! [`render`] walks the template once and never re-scans what it wrote, so a
//! value containing `{admin_password}` stays literal instead of resolving to
//! another variable. That is why it is hand-written rather than a chain of
//! `str::replace`, which does re-scan and depends on order.
//!
//! Values landing in the `[html]` part are HTML-escaped: the markup belongs
//! to the server, the values may come from a player.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::error::{EmailError, Fail};
use crate::message::Var;

/// `{{` and `}}` are literal braces, so a template can contain CSS or JSON.
const ESCAPE_OPEN: &str = "{{";
const ESCAPE_CLOSE: &str = "}}";

#[derive(Debug, Clone, Default)]
pub struct Template {
    pub subject: String,
    pub text: String,
    pub html: String,
}

/// Substitutes `{key}` occurrences in `source` from `vars`.
///
/// An unknown key is left in place, braces and all. That is deliberate: a
/// typo'd `{naem}` arriving visibly in the mail is a bug someone reports,
/// whereas silently substituting an empty string produces a mail that reads
/// almost right and is never noticed.
pub fn render(source: &str, vars: &HashMap<String, Var>, html: bool) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;

    while let Some(idx) = rest.find(['{', '}']) {
        out.push_str(&rest[..idx]);
        let tail = &rest[idx..];

        if let Some(after) = tail.strip_prefix(ESCAPE_OPEN) {
            out.push('{');
            rest = after;
            continue;
        }
        if let Some(after) = tail.strip_prefix(ESCAPE_CLOSE) {
            out.push('}');
            rest = after;
            continue;
        }

        // A lone '}' is not a placeholder; emit it and move on.
        if let Some(after) = tail.strip_prefix('}') {
            out.push('}');
            rest = after;
            continue;
        }

        // '{' with no closing brace: the rest of the template is literal.
        let Some(end) = tail.find('}') else {
            out.push_str(tail);
            return out;
        };

        let key = &tail[1..end];
        match vars.get(key) {
            // The value is pushed verbatim and `rest` advances past the whole
            // placeholder, so nothing inside the value is ever examined again.
            Some(var) if html && !var.raw => escape_html(&var.text, &mut out),
            Some(var) => out.push_str(&var.text),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }

    out.push_str(rest);
    out
}

/// Writes `value` with the five characters that mean something in HTML
/// replaced by their entities.
fn escape_html(value: &str, out: &mut String) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
}

/// Reads a template file.
///
/// A plain HTML file is a template too — that is what most people already
/// have, exported from a designer or copied from a PHP project:
///
/// ```text
/// <html><head><title>Welcome, {name}</title></head>
/// <body><p>Your account on {server} is ready.</p></body></html>
/// ```
///
/// With no `[section]` header anywhere, the whole file is one body: HTML when
/// the name ends in `.html`/`.htm`, plain text otherwise. An HTML file's
/// `<title>` becomes the subject, since that is where a subject naturally
/// lives in a document.
pub fn parse(contents: &str, path: &Path) -> Template {
    if has_sections(contents) {
        return parse_sections(contents);
    }

    let html = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(ext) if ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm")
    );
    if !html {
        return Template {
            text: contents.trim_end_matches('\n').to_string(),
            ..Template::default()
        };
    }

    Template {
        subject: title_of(contents),
        html: contents.trim_end_matches('\n').to_string(),
        ..Template::default()
    }
}

/// Whether the file uses the `[section]` format. A line that is exactly one
/// of the known headers — an HTML file has no such line, and a `[` inside
/// markup or text is never alone on its line.
fn has_sections(contents: &str) -> bool {
    contents.lines().any(|line| {
        matches!(
            line.trim().to_ascii_lowercase().as_str(),
            "[subject]" | "[text]" | "[body]" | "[plain]" | "[html]"
        )
    })
}

/// The contents of `<title>`, with surrounding whitespace dropped.
fn title_of(contents: &str) -> String {
    let lower = contents.to_ascii_lowercase();
    let Some(open) = lower.find("<title>") else {
        return String::new();
    };
    let start = open + "<title>".len();
    let Some(end) = lower[start..].find("</title>") else {
        return String::new();
    };
    contents[start..start + end].trim().to_string()
}

/// Parses the `[section]` file format:
///
/// ```text
/// [subject]
/// Welcome to the server, {name}!
///
/// [text]
/// Hello {name}, your account is ready.
///
/// [html]
/// <p>Hello <b>{name}</b>, your account is ready.</p>
/// ```
///
/// Sections may appear in any order and any of them may be omitted. Everything
/// before the first section header is ignored, which makes room for a comment
/// block at the top of the file.
fn parse_sections(contents: &str) -> Template {
    let mut tpl = Template::default();
    let mut current: Option<&str> = None;
    let mut buffer = String::new();

    // Bodies are copied byte-for-byte apart from the trailing newline, so
    // indentation and blank lines in an HTML body survive intact.
    let flush = |section: Option<&str>, buffer: &mut String, tpl: &mut Template| {
        let Some(section) = section else {
            buffer.clear();
            return;
        };
        let value = buffer.trim_matches('\n').to_string();
        match section {
            "subject" => tpl.subject = value.trim().to_string(),
            "text" => tpl.text = value,
            "html" => tpl.html = value,
            _ => {}
        }
        buffer.clear();
    };

    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            flush(current, &mut buffer, &mut tpl);
            current = Some(match name.trim().to_ascii_lowercase().as_str() {
                "subject" => "subject",
                "text" | "body" | "plain" => "text",
                "html" => "html",
                // An unrecognised section is swallowed rather than appended to
                // whatever came before it.
                _ => "",
            });
            continue;
        }

        if current.is_some() {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }

    flush(current, &mut buffer, &mut tpl);
    tpl
}

/// Templates, keyed by the path they were read from.
///
/// There is deliberately no template *handle*. A template is a file, and a
/// file already has a name — wrapping it in a create/use/destroy lifecycle
/// added three natives and a lifetime to get wrong, and bought nothing: a
/// gamemode never has two different templates at the same path.
///
/// The cache reloads when the file changes on disk (size or mtime), so editing
/// the wording of a welcome mail takes effect on the next send with no
/// reload, no `/gmx` and no restart. The `stat` that checks it costs
/// microseconds and happens once per message, on the main thread.
pub struct TemplateCache {
    entries: HashMap<PathBuf, CachedTemplate>,
}

struct CachedTemplate {
    template: Arc<Template>,
    /// What the file looked like when it was read. `mtime` is unavailable on
    /// some filesystems, hence the `Option`; length alone still catches most
    /// edits, and a stale template is a cosmetic problem, not a correctness
    /// one.
    mtime: Option<SystemTime>,
    len: u64,
}

impl TemplateCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Returns the parsed template, reading it from disk if it is new or has
    /// changed.
    ///
    /// The `Arc` is what lets a worker thread render the template without the
    /// cache: the draft carries its own reference, so a reload here never
    /// disturbs a message already in flight.
    /// Drops every cached template, so the next send reads from disk again.
    /// Returns how many were dropped.
    ///
    /// The cache reloads on its own when a file's size or mtime changes, so
    /// this is for the cases that hides: an editor that restores the mtime, a
    /// file replaced by a deploy script that preserves it, a network share
    /// with a coarse clock.
    pub fn clear(&mut self) -> usize {
        let count = self.entries.len();
        self.entries.clear();
        count
    }

    pub fn get(&mut self, path: &Path) -> Result<Arc<Template>, Fail> {
        // Keyed by the resolved path, so two spellings of one file share an
        // entry and a symlink swapped to point elsewhere is re-checked.
        // Keyed by the checked path, so two spellings of one file share an
        // entry and a symlink swapped to point elsewhere is re-checked.
        let path = &crate::sandbox::resolve(path, EmailError::TemplateFailed)?;
        let unreadable = |e: std::io::Error| {
            EmailError::TemplateFailed.because(format!(
                "could not read the template '{}': {e}",
                path.display()
            ))
        };
        let meta = std::fs::metadata(path).map_err(unreadable)?;

        let mtime = meta.modified().ok();
        let len = meta.len();

        if let Some(cached) = self.entries.get(path)
            && cached.len == len
            && cached.mtime == mtime
        {
            return Ok(cached.template.clone());
        }

        let bytes = crate::sandbox::read(path, EmailError::TemplateFailed)?;
        let contents = String::from_utf8(bytes).map_err(|_| {
            EmailError::TemplateFailed.because(format!(
                "the template '{}' is not valid UTF-8 (save it as UTF-8)",
                path.display()
            ))
        })?;

        let template = Arc::new(parse(&contents, path));
        self.entries.insert(
            path.to_path_buf(),
            CachedTemplate {
                template: template.clone(),
                mtime,
                len,
            },
        );

        Ok(template)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, Var> {
        pairs
            .iter()
            .map(|(k, v)| {
                (
                    (*k).to_string(),
                    Var {
                        text: (*v).to_string(),
                        raw: false,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn substitutes_a_known_key() {
        let v = vars(&[("name", "Erick")]);
        assert_eq!(render("Hello {name}!", &v, false), "Hello Erick!");
    }

    #[test]
    fn substitutes_the_same_key_twice() {
        let v = vars(&[("n", "x")]);
        assert_eq!(render("{n}-{n}", &v, false), "x-x");
    }

    #[test]
    fn an_unknown_key_is_left_visible() {
        let v = vars(&[("name", "Erick")]);
        assert_eq!(render("Hello {naem}!", &v, false), "Hello {naem}!");
    }

    #[test]
    fn a_substituted_value_is_never_re_expanded() {
        // The whole point of the single pass: `name` must not act as a way to
        // read `secret`.
        let v = vars(&[("name", "{secret}"), ("secret", "hunter2")]);
        assert_eq!(render("Hi {name}", &v, false), "Hi {secret}");
    }

    #[test]
    fn a_self_referential_value_does_not_loop() {
        let v = vars(&[("a", "{a}")]);
        assert_eq!(render("{a}", &v, false), "{a}");
    }

    #[test]
    fn doubled_braces_are_literal() {
        let v = vars(&[("name", "Erick")]);
        assert_eq!(
            render("{{ color: red }} for {name}", &v, false),
            "{ color: red } for Erick"
        );
    }

    #[test]
    fn an_unclosed_brace_is_literal_text() {
        let v = vars(&[("name", "Erick")]);
        assert_eq!(
            render("Hello {name and {oops", &v, false),
            "Hello {name and {oops"
        );
    }

    #[test]
    fn a_stray_closing_brace_is_literal() {
        assert_eq!(render("a } b", &HashMap::new(), false), "a } b");
    }

    #[test]
    fn an_empty_placeholder_is_left_alone() {
        assert_eq!(render("a {} b", &HashMap::new(), false), "a {} b");
    }

    #[test]
    fn text_without_placeholders_is_unchanged() {
        assert_eq!(render("plain text", &HashMap::new(), false), "plain text");
    }

    #[test]
    fn parses_all_three_sections() {
        let tpl = parse_sections(
            "; a comment above everything\n\
             [subject]\n\
             Welcome, {name}!\n\
             \n\
             [text]\n\
             Hello {name}.\n\
             \n\
             [html]\n\
             <p>Hello <b>{name}</b>.</p>\n",
        );
        assert_eq!(tpl.subject, "Welcome, {name}!");
        assert_eq!(tpl.text, "Hello {name}.");
        assert_eq!(tpl.html, "<p>Hello <b>{name}</b>.</p>");
    }

    #[test]
    fn sections_may_be_reordered_and_omitted() {
        let tpl = parse_sections("[html]\n<p>hi</p>\n[subject]\nHi\n");
        assert_eq!(tpl.subject, "Hi");
        assert_eq!(tpl.html, "<p>hi</p>");
        assert!(tpl.text.is_empty());
    }

    #[test]
    fn body_and_plain_are_aliases_for_text() {
        assert_eq!(parse_sections("[body]\nx\n").text, "x");
        assert_eq!(parse_sections("[plain]\ny\n").text, "y");
    }

    #[test]
    fn an_unknown_section_is_discarded_not_appended() {
        let tpl = parse_sections("[text]\nkeep\n[footer]\ndrop\n");
        assert_eq!(tpl.text, "keep");
    }

    #[test]
    fn a_body_keeps_its_internal_blank_lines_and_indentation() {
        let tpl = parse_sections("[text]\nline one\n\n    indented\n");
        assert_eq!(tpl.text, "line one\n\n    indented");
    }

    #[test]
    fn a_template_is_read_and_then_served_from_cache() {
        let dir = crate::sandbox::test_dir("tpl_cache");
        let path = dir.join("welcome.tpl");
        std::fs::write(&path, "[subject]\nHi {name}\n").expect("write");

        let mut cache = TemplateCache::new();
        let first = cache.get(&path).expect("reads");
        assert_eq!(first.subject, "Hi {name}");

        let second = cache.get(&path).expect("reads");
        // Same allocation: the second call did not re-read the file.
        assert!(Arc::ptr_eq(&first, &second));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clearing_the_cache_forces_the_next_read() {
        let dir = crate::sandbox::test_dir("tpl_clear");
        let path = dir.join("welcome.tpl");
        std::fs::write(&path, "[subject]\nfirst\n").expect("write");

        let mut cache = TemplateCache::new();
        let first = cache.get(&path).expect("reads");
        assert_eq!(cache.clear(), 1);
        assert_eq!(cache.clear(), 0);

        // Same file, same size and mtime: only the clear makes it read again.
        let second = cache.get(&path).expect("reads");
        assert!(!Arc::ptr_eq(&first, &second));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn editing_the_file_invalidates_the_cache() {
        let dir = crate::sandbox::test_dir("tpl_reload");
        let path = dir.join("welcome.tpl");
        std::fs::write(&path, "[subject]\nold\n").expect("write");

        let mut cache = TemplateCache::new();
        assert_eq!(cache.get(&path).expect("reads").subject, "old");

        // A different length is enough to invalidate even where the
        // filesystem's mtime resolution would hide a same-second edit.
        std::fs::write(&path, "[subject]\nbrand new wording\n").expect("rewrite");
        assert_eq!(
            cache.get(&path).expect("reads").subject,
            "brand new wording"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn html_values_are_escaped_and_text_values_are_not() {
        let v = vars(&[("name", "<b>\"Bob\" & 'co'</b>")]);
        assert_eq!(
            render("<p>{name}</p>", &v, true),
            "<p>&lt;b&gt;&quot;Bob&quot; &amp; &#39;co&#39;&lt;/b&gt;</p>"
        );
        assert_eq!(render("{name}", &v, false), "<b>\"Bob\" & 'co'</b>");
    }

    #[test]
    fn a_plain_html_file_is_a_template_with_its_title_as_the_subject() {
        let tpl = parse(
            "<html><head><title> Welcome, {name} </title></head>\n<body><p>Hi {name}</p></body></html>\n",
            Path::new("welcome.html"),
        );
        assert_eq!(tpl.subject, "Welcome, {name}");
        assert!(tpl.html.contains("<body><p>Hi {name}</p></body>"));
        assert!(tpl.text.is_empty());
    }

    #[test]
    fn an_html_file_without_a_title_just_has_no_subject() {
        let tpl = parse("<p>Hi {name}</p>", Path::new("mail.HTM"));
        assert!(tpl.subject.is_empty());
        assert_eq!(tpl.html, "<p>Hi {name}</p>");
    }

    #[test]
    fn a_file_with_no_sections_and_no_html_extension_is_plain_text() {
        let tpl = parse("Hello {name}, welcome.\n", Path::new("welcome.txt"));
        assert_eq!(tpl.text, "Hello {name}, welcome.");
        assert!(tpl.html.is_empty());
    }

    #[test]
    fn sections_win_over_the_extension() {
        // A .html file that does use sections is read as sections.
        let tpl = parse(
            "[subject]\nHi\n\n[html]\n<p>x</p>\n",
            Path::new("mail.html"),
        );
        assert_eq!(tpl.subject, "Hi");
        assert_eq!(tpl.html, "<p>x</p>");
    }

    #[test]
    fn a_raw_value_is_the_one_thing_that_is_not_escaped() {
        let mut v = vars(&[("safe", "<b>x</b>")]);
        v.insert(
            "markup".into(),
            Var {
                text: "<b>x</b>".into(),
                raw: true,
            },
        );
        assert_eq!(render("{safe}", &v, true), "&lt;b&gt;x&lt;/b&gt;");
        assert_eq!(render("{markup}", &v, true), "<b>x</b>");
    }

    #[test]
    fn a_template_outside_the_server_folder_is_refused() {
        let mut cache = TemplateCache::new();
        let Err((code, _)) = cache.get(Path::new("../../../../../../etc/hostname")) else {
            panic!("expected a template outside the folder to be refused");
        };
        assert_eq!(code, EmailError::TemplateFailed);
    }

    #[test]
    fn a_missing_template_names_the_path() {
        let mut cache = TemplateCache::new();
        let Err((code, message)) = cache.get(Path::new("/definitely/not/here.tpl")) else {
            panic!("expected a missing template to fail");
        };
        assert_eq!(code, EmailError::TemplateFailed);
        assert!(message.contains("here.tpl"));
    }
}
