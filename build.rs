fn main() {
    let output = std::process::Command::new("date")
        .arg("+%b %d %Y|%H:%M:%S|%Y")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok());

    let (date, time, year) = match output {
        Some(s) => {
            let s = s.trim().to_string();
            let parts: Vec<&str> = s.splitn(3, '|').collect();
            (
                parts.first().unwrap_or(&"Unknown").to_string(),
                parts.get(1).unwrap_or(&"Unknown").to_string(),
                parts.get(2).unwrap_or(&"Unknown").to_string(),
            )
        }
        None => (
            "Unknown".to_string(),
            "Unknown".to_string(),
            "Unknown".to_string(),
        ),
    };

    println!("cargo:rustc-env=BUILD_DATE={date}");
    println!("cargo:rustc-env=BUILD_TIME={time}");
    println!("cargo:rustc-env=BUILD_YEAR={year}");

    // Declare the script's real inputs. Without this, Cargo assumes the build
    // script depends on every file in the package, so editing a generated .inc,
    // an example, or a doc re-runs the script and invalidates the Rust
    // build/test cache for no reason. Listing the inputs scopes the re-run to
    // what actually feeds the generation: the template, this script, and the
    // manifest (for the version). A clean build (releases) still runs the
    // script, so BUILD_* reflect that build; incremental dev builds keep the
    // BUILD_* from the last input change, which is fine for the banner.
    println!("cargo:rerun-if-changed=include/email_samp.inc.in");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    generate_inc();
}

fn generate_inc() {
    use std::fs;

    let template_path = "include/email_samp.inc.in";
    let output_path = "include/email_samp.inc";

    // The write below is idempotent: it only touches disk when the rendered
    // output actually differs, so a re-run never churns timestamps (which would
    // otherwise cascade back into the build).

    let template = fs::read_to_string(template_path)
        .unwrap_or_else(|e| panic!("failed to read {template_path}: {e}"));

    let version = env!("CARGO_PKG_VERSION");
    let rendered = template.replace("{{VERSION}}", version);

    if fs::read_to_string(output_path).ok().as_deref() != Some(rendered.as_str()) {
        fs::write(output_path, &rendered)
            .unwrap_or_else(|e| panic!("failed to write {output_path}: {e}"));
    }

    generate_omp_inc(&rendered);
}

/// Generates `email_samp_omp.inc` - a **standalone** include exposing the API
/// under open.mp-style `Prefix_PascalCase` names.
///
/// Self-contained on purpose: the two includes are alternatives (you write one
/// style or the other, never both in the same script), so this carries its own
/// copy of the enums, the `EMAIL_SAMP_VERSION` define and the `OnQueryError`
/// forward rather than pulling in the base. Only the natives differ - each
/// becomes `native Styled(...) = real;`, a Pawn alias to the real snake_case
/// native, which resolves at runtime with no cost and no plugin-side change
/// (the target need not even be declared, so the snake_case names are absent).
///
/// Derived from the base `.inc` on every build, so the two never drift: a
/// native, enum or constant added to the template appears here automatically.
fn generate_omp_inc(base_rendered: &str) {
    use std::fmt::Write;
    use std::fs;

    let output_path = "include/email_samp_omp.inc";

    let mut out = String::from(
        "/*\n\
         \x20* email_samp - open.mp naming style (standalone).\n\
         \x20*\n\
         \x20* GENERATED from email_samp.inc by build.rs - do not edit by hand.\n\
         \x20*\n\
         \x20* Include THIS instead of <email_samp> to write the API in\n\
         \x20* open.mp's Prefix_PascalCase style (Email_Connect, Email_AddTo,\n\
         \x20* Email_SetSubject). Each native aliases the real one, so there is no\n\
         \x20* runtime cost and nothing changes on the plugin side. This file is\n\
         \x20* self-contained - do not include it together with <email_samp>.\n\
         \x20*/\n\n\
         #if defined _email_samp_omp_included\n    #endinput\n#endif\n\
         #define _email_samp_omp_included\n",
    );

    // Everything after the base guard: the version define, the enums, the
    // forward and the natives. The natives are rewritten as aliases; the rest
    // is copied verbatim, so enums/defines/forward stay a single source.
    let marker = "#define _email_samp_included";
    let body = match base_rendered.find(marker) {
        Some(i) => &base_rendered[i + marker.len()..],
        None => base_rendered,
    };

    for line in body.lines() {
        let trimmed = line.trim();

        let Some(rest) = trimmed.strip_prefix("native ") else {
            // Not a native declaration: copied, with every `email_*` name
            // renamed. That keeps a stock's body calling natives that exist
            // under this include, and the usage shown in comments in the
            // style the reader chose.
            out.push_str(&rename_identifiers(line));
            out.push('\n');
            continue;
        };

        let rest = rest.trim().trim_end_matches(';').trim();
        let (Some(open), Some(close)) = (rest.find('('), rest.rfind(')')) else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let head = rest[..open].trim(); // optional `tag:` plus the native name
        let params = &rest[open + 1..close];

        let (tag, name) = match head.rfind(':') {
            Some(i) => (&head[..=i], head[i + 1..].trim()),
            None => ("", head),
        };

        let styled = to_omp_name(name);
        // The right-hand side is the real native the plugin registered; the
        // left-hand side is the alias the gamemode writes.
        let _ = writeln!(out, "native {tag}{styled}({params}) = {name};");
    }

    if fs::read_to_string(output_path).ok().as_deref() != Some(out.as_str()) {
        fs::write(output_path, &out)
            .unwrap_or_else(|e| panic!("failed to write {output_path}: {e}"));
    }
}

/// Renames every `email_*` identifier in `line` to its open.mp form.
///
/// `email_samp` itself (the library, the include, the guard) is a name of the
/// plugin, not of an API function, and stays as it is.
fn rename_identifiers(line: &str) -> String {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(line.len());
    let mut rest = line;

    while let Some(at) = rest.find("email_") {
        let boundary = rest[..at].chars().next_back().is_none_or(|c| !is_ident(c));
        let len = rest[at..]
            .find(|c: char| !is_ident(c))
            .unwrap_or(rest.len() - at);
        let word = &rest[at..at + len];

        out.push_str(&rest[..at]);
        if boundary && !word.starts_with("email_samp") {
            out.push_str(&to_omp_name(word));
        } else {
            out.push_str(word);
        }
        rest = &rest[at + len..];
    }
    out.push_str(rest);
    out
}

/// `email_add_to` -> `Email_AddTo`, `email_set_reply_to` -> `Email_SetReplyTo`.
///
/// The first segment is the group prefix; the rest become one PascalCase word.
///
/// There is no exception table: every native name splits cleanly on `_` into
/// whole words, so the mechanical conversion is already the name open.mp
/// would write. Keep it that way when
/// adding natives — a name like `email_pquery` would need an entry, a name
/// like `email_parallel_send` would not.
fn to_omp_name(name: &str) -> String {
    let mut segments = name.split('_');

    let prefix = match segments.next().unwrap_or("") {
        "email" => "Email".to_string(),
        other => capitalize(other),
    };

    let rest: String = segments.map(capitalize).collect();
    if rest.is_empty() {
        prefix
    } else {
        format!("{prefix}_{rest}")
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
