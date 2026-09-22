//! The `%d` / `%s` / `%f` formatting `email_set_var` accepts.
//!
//! A template variable is always text in the end, so the value is written the
//! way Pawn already writes text — the same specifiers as `format` and
//! `printf`. That is what lets one native take a nickname, a slot count and a
//! balance without the gamemode formatting anything first.
//!
//! Parsing the specifiers is separate from filling them in: the plugin reads
//! each argument off the AMX stack as the type its specifier names, which is
//! the only type information a variadic call carries.

/// A piece of the value: literal text, or a hole waiting for an argument.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(String),
    Int,
    /// Decimal places; `None` is Pawn's default of six.
    Float(Option<usize>),
    Str,
    /// `%r`: a string that is NOT escaped when it lands in HTML.
    Raw,
}

/// What a [`Piece`] hole was filled with.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32),
    Float(f32),
    Str(String),
}

/// Splits a value into literal text and holes.
///
/// An unknown specifier is kept as written: `100%` and `50% off` are text a
/// gamemode really does pass, and turning those into an error would be worse
/// than printing them.
pub fn parse(source: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '%' {
            text.push(ch);
            continue;
        }

        // Read an optional `.N` precision between the % and the specifier.
        let mut spec = String::from("%");
        let mut precision: Option<usize> = None;
        if chars.peek() == Some(&'.') {
            spec.push(chars.next().unwrap_or('.'));
            let mut digits = String::new();
            while chars.peek().is_some_and(char::is_ascii_digit) {
                let digit = chars.next().unwrap_or('0');
                spec.push(digit);
                digits.push(digit);
            }
            precision = digits.parse().ok();
        }

        let piece = match chars.next() {
            Some('%') => {
                text.push('%');
                continue;
            }
            Some('d' | 'i') if precision.is_none() => Piece::Int,
            Some('f') => Piece::Float(precision),
            Some('s') if precision.is_none() => Piece::Str,
            Some('r') if precision.is_none() => Piece::Raw,
            // Not a specifier this plugin knows: keep the characters.
            other => {
                text.push_str(&spec);
                text.extend(other);
                continue;
            }
        };

        if !text.is_empty() {
            pieces.push(Piece::Text(std::mem::take(&mut text)));
        }
        pieces.push(piece);
    }

    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    pieces
}

/// Whether any piece turns escaping off for the whole value.
pub fn is_raw(pieces: &[Piece]) -> bool {
    pieces.contains(&Piece::Raw)
}

/// How many arguments the pieces expect.
pub fn holes(pieces: &[Piece]) -> usize {
    pieces
        .iter()
        .filter(|p| !matches!(p, Piece::Text(_)))
        .count()
}

/// Fills the holes in order. A hole with no value left is dropped rather than
/// guessed at: the gamemode wrote more specifiers than arguments.
pub fn render(pieces: &[Piece], values: &[Value]) -> String {
    let mut out = String::new();
    let mut values = values.iter();

    for piece in pieces {
        if let Piece::Text(text) = piece {
            out.push_str(text);
            continue;
        }
        let Some(value) = values.next() else {
            continue;
        };
        match (piece, value) {
            (Piece::Float(places), Value::Float(number)) => {
                let places = places.unwrap_or(6);
                out.push_str(&format!("{number:.places$}"));
            }
            (_, Value::Int(number)) => out.push_str(&number.to_string()),
            (_, Value::Float(number)) => out.push_str(&number.to_string()),
            (_, Value::Str(text)) => out.push_str(text),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Piece {
        Piece::Text(s.to_string())
    }

    #[test]
    fn a_value_with_no_specifiers_is_one_piece_of_text() {
        assert_eq!(parse("Erick"), vec![text("Erick")]);
        assert_eq!(holes(&parse("Erick")), 0);
    }

    #[test]
    fn the_specifiers_become_holes() {
        assert_eq!(
            parse("%s has %d slots"),
            vec![Piece::Str, text(" has "), Piece::Int, text(" slots")]
        );
        assert_eq!(holes(&parse("%s has %d slots")), 2);
    }

    #[test]
    fn precision_is_read_for_floats() {
        assert_eq!(parse("%.2f"), vec![Piece::Float(Some(2))]);
        assert_eq!(parse("%f"), vec![Piece::Float(None)]);
    }

    #[test]
    fn a_percent_sign_in_ordinary_text_survives() {
        // Values a gamemode really passes: neither is a format specifier.
        assert_eq!(parse("100% free"), vec![text("100% free")]);
        assert_eq!(parse("50%% off"), vec![text("50% off")]);
        assert_eq!(holes(&parse("100% free")), 0);
    }

    #[test]
    fn only_the_r_specifier_turns_escaping_off() {
        assert!(is_raw(&parse("%r")));
        assert!(is_raw(&parse("rows: %r")));
        assert!(!is_raw(&parse("%s %d %.2f")));
        assert!(!is_raw(&parse("plain text")));
    }

    #[test]
    fn numbers_and_text_are_written_in_order() {
        let pieces = parse("%s has %d and %.2f");
        let values = [
            Value::Str("Erick".into()),
            Value::Int(200),
            Value::Float(1250.5),
        ];
        assert_eq!(render(&pieces, &values), "Erick has 200 and 1250.50");
    }

    #[test]
    fn a_float_without_precision_uses_pawns_six_places() {
        assert_eq!(render(&parse("%f"), &[Value::Float(1.5)]), "1.500000");
    }

    #[test]
    fn a_hole_with_no_argument_is_dropped() {
        assert_eq!(
            render(&parse("%s and %s"), &[Value::Str("a".into())]),
            "a and "
        );
    }
}
