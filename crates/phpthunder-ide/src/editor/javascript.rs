//! JavaScript inside markup: `<script>` blocks and the expressions Alpine
//! (`x-data`, `@click`, `:class`) and Livewire (`wire:click`) attributes
//! hold. A small scanner, like the Blade one: keywords, strings and
//! template literals, numbers, comments, calls, `this` and Alpine's
//! `$magics`.

use super::*;

/// JavaScript keywords and literal words.
const KEYWORDS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "of",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "from",
    "as",
    "get",
    "set",
];

/// A scanned span: byte range of the document and its token.
type Span = (usize, usize, Token);

/// Whether `b` can be part of a JavaScript name.
fn is_js_name(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Offset just past the string or template literal whose quote is at `at`,
/// bounded by `end`.
fn literal_end(b: &[u8], at: usize, end: usize) -> usize {
    let quote = b[at];
    let mut i = at + 1;
    while i < end {
        match b[i] {
            b'\\' => i += 2,
            c if c == quote => return i + 1,
            // A plain string stops at the line end; a template does not.
            b'\n' if quote != b'`' => return i,
            _ => i += 1,
        }
    }
    end
}

/// Highlights the JavaScript in `start..end` of `src`, appending spans in
/// document order. Used for `<script>` contents and Alpine attribute values.
pub(super) fn scan_javascript(src: &str, start: usize, end: usize, out: &mut Vec<Span>) {
    let b = src.as_bytes();
    let end = end.min(b.len());
    let mut i = start;
    while i < end {
        let c = b[i];
        let rest = &b[i..end];
        if rest.starts_with(b"//") {
            let stop = src[i..end].find('\n').map_or(end, |n| i + n);
            out.push((i, stop, Token::Comment));
            i = stop;
        } else if rest.starts_with(b"/*") {
            let stop = src[i + 2..end].find("*/").map_or(end, |n| i + 2 + n + 2);
            out.push((i, stop, Token::Comment));
            i = stop;
        } else if matches!(c, b'\'' | b'"' | b'`') {
            let stop = literal_end(b, i, end);
            out.push((i, stop, Token::String));
            i = stop;
        } else if c.is_ascii_digit() && (i == 0 || !is_js_name(b[i - 1])) {
            let s = i;
            while i < end && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            out.push((s, i, Token::Number));
        } else if rest.starts_with(b"=>") || rest.starts_with(b"?.") {
            out.push((i, i + 2, Token::Punct));
            i += 2;
        } else if matches!(c, b',' | b';') {
            out.push((i, i + 1, Token::Separator));
            i += 1;
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            let s = i;
            while i < end && is_js_name(b[i]) {
                i += 1;
            }
            let word = &src[s..i];
            let after_dot = s > 0 && b[s - 1] == b'.';
            let mut k = i;
            while k < end && b[k] == b' ' {
                k += 1;
            }
            let token = if word.starts_with('$') || (word == "this" && !after_dot) {
                Some(Token::Variable)
            } else if KEYWORDS.contains(&word) && !after_dot {
                Some(Token::Keyword)
            } else if b.get(k) == Some(&b'(') && k < end {
                Some(Token::Function)
            } else if word.starts_with(|c: char| c.is_ascii_uppercase()) && !after_dot {
                Some(Token::Type)
            } else {
                None
            };
            if let Some(token) = token {
                out.push((s, i, token));
            }
        } else {
            i += 1;
        }
    }
}

/// Whether an attribute name holds JavaScript: Alpine directives (`x-…`
/// except `x-ref`/`x-transition…`), event (`@…`, `x-on:…`) and bind
/// (`:…`) shorthands, and Livewire `wire:` actions.
pub(super) fn is_script_attribute(name: &str) -> bool {
    let plain = matches!(
        name,
        "x-ref" | "x-cloak" | "x-ignore" | "x-teleport" | "x-id"
    ) || name.starts_with("x-transition")
        || name.starts_with("wire:model")
        || name.starts_with("wire:key")
        || name.starts_with("wire:loading")
        || name.starts_with("wire:target")
        || name.starts_with("wire:poll")
        || name.starts_with("wire:navigate");
    !plain
        && (name.starts_with("x-")
            || name.starts_with('@')
            || name.starts_with(':')
            || name.starts_with("wire:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(src: &str) -> Vec<(&str, Token)> {
        let mut out = Vec::new();
        scan_javascript(src, 0, src.len(), &mut out);
        out.into_iter().map(|(s, e, t)| (&src[s..e], t)).collect()
    }

    #[test]
    fn scan_javascript_should_mark_keywords_strings_and_magics() {
        let got = tokens("open = !open; $dispatch('saved', { id: 1 }) // done");
        assert!(got.contains(&("$dispatch", Token::Variable)));
        assert!(got.contains(&("'saved'", Token::String)));
        assert!(got.contains(&("1", Token::Number)));
        assert!(got.contains(&("// done", Token::Comment)));
    }

    #[test]
    fn is_script_attribute_should_know_alpine_and_livewire() {
        assert!(is_script_attribute("x-data"));
        assert!(is_script_attribute("@click.prevent"));
        assert!(is_script_attribute(":class"));
        assert!(!is_script_attribute("x-ref"));
        assert!(!is_script_attribute("class"));
    }
}
