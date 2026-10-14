//! Blade template highlighting: directives, echo tags, Blade and HTML
//! comments, HTML tags and attributes, the PHP expressions inside echoes,
//! directive arguments and `@php` blocks, and the JavaScript in `<script>`
//! blocks and Alpine / Livewire attribute values. A hand-written scanner:
//! Blade mixes HTML and PHP in ways a PHP grammar does not parse.

use super::*;

/// PHP words colored as keywords inside Blade expressions.
const KEYWORDS: &[&str] = &[
    "true",
    "false",
    "null",
    "and",
    "or",
    "not",
    "new",
    "instanceof",
    "fn",
    "function",
    "as",
    "foreach",
    "if",
    "else",
    "elseif",
    "return",
    "use",
    "match",
    "static",
    "self",
    "array",
    "isset",
    "empty",
];

/// A scanned span: byte range of the document and its token.
type Span = (usize, usize, Token);

/// Computes highlight spans for a whole Blade template. Called by
/// `Document::refresh_highlights` for `.blade.php` files.
pub fn compute_blade_highlights(src: &str) -> LineSpans {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(src.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let mut lines: LineSpans = vec![Vec::new(); starts.len()];
    let mut spans = Vec::new();
    scan_template(src, &mut spans);
    for (start, end, token) in spans {
        push_span(&mut lines, &starts, src, start, end, token);
    }
    lines
}

/// Offset just past the first `needle` at or after `from`, or the end of `src`.
fn after(src: &str, from: usize, needle: &str) -> (usize, usize) {
    match src[from..].find(needle) {
        Some(i) => (from + i, from + i + needle.len()),
        None => (src.len(), src.len()),
    }
}

/// Whether `b` can be part of a name.
fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Scans template text, appending spans in document order.
pub(super) fn scan_template(src: &str, out: &mut Vec<Span>) {
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // Bytes, not `&src[i..]`: `i` may sit inside a multi-byte character.
        let rest = &b[i..];
        if rest.starts_with(b"{{--") {
            let (_, end) = after(src, i + 4, "--}}");
            out.push((i, end, Token::Comment));
            i = end;
        } else if rest.starts_with(b"<!--") {
            let (_, end) = after(src, i + 4, "-->");
            out.push((i, end, Token::Comment));
            i = end;
        } else if rest.starts_with(b"{!!") || rest.starts_with(b"{{") {
            let (open, close) = if rest.starts_with(b"{!!") {
                ("{!!", "!!}")
            } else {
                ("{{", "}}")
            };
            out.push((i, i + open.len(), Token::Keyword));
            let (inner_end, end) = after(src, i + open.len(), close);
            scan_expression(src, i + open.len(), inner_end, out);
            if inner_end < end {
                out.push((inner_end, end, Token::Keyword));
            }
            i = end;
        } else if b[i] == b'@' {
            i = scan_directive(src, i, out);
        } else if b[i] == b'<'
            && b.get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/')
        {
            let end = scan_tag(src, i, out);
            // `<script>` contents are JavaScript, up to `</script>`.
            let is_script = src
                .get(i + 1..i + 7)
                .is_some_and(|t| t.eq_ignore_ascii_case("script"))
                && b.get(i + 7)
                    .is_some_and(|c| !is_name_byte(*c) && *c != b'-');
            i = end;
            if is_script {
                let close = src[i..].find("</script").map_or(src.len(), |n| i + n);
                super::javascript::scan_javascript(src, i, close, out);
                i = close;
            }
        } else {
            i += 1;
        }
    }
}

/// Scans `@name(...)` or an `@php … @endphp` block starting at `at`;
/// returns where scanning resumes. `@@` and `user@example` are text.
fn scan_directive(src: &str, at: usize, out: &mut Vec<Span>) -> usize {
    let b = src.as_bytes();
    if b.get(at + 1) == Some(&b'@') {
        return at + 2;
    }
    if at > 0 && is_name_byte(b[at - 1]) {
        return at + 1;
    }
    let mut j = at + 1;
    while j < b.len() && is_name_byte(b[j]) {
        j += 1;
    }
    if j == at + 1 {
        return at + 1;
    }
    out.push((at, j, Token::Keyword));
    let mut k = j;
    while k < b.len() && b[k] == b' ' {
        k += 1;
    }
    if b.get(k) == Some(&b'(') {
        let close = matching_paren(src, k);
        scan_expression(src, k + 1, close, out);
        return (close + 1).min(b.len());
    }
    if &src[at + 1..j] == "php" {
        let (inner_end, end) = after(src, j, "@endphp");
        scan_expression(src, j, inner_end, out);
        if inner_end < end {
            out.push((inner_end, end, Token::Keyword));
        }
        return end;
    }
    j
}

/// Offset of the `)` matching the `(` at `open`, skipping strings; the end
/// of `src` when unbalanced.
fn matching_paren(src: &str, open: usize) -> usize {
    let b = src.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'\'' | b'"' => i = string_end(src, i) - 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// Offset just past the string literal whose quote is at `at`.
fn string_end(src: &str, at: usize) -> usize {
    let b = src.as_bytes();
    let quote = b[at];
    let mut i = at + 1;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    b.len()
}

/// Scans an HTML tag starting at `<`: the tag name, attribute names and
/// quoted values. Returns the offset after `>`.
fn scan_tag(src: &str, at: usize, out: &mut Vec<Span>) -> usize {
    let b = src.as_bytes();
    let mut i = at + 1;
    if b.get(i) == Some(&b'/') {
        i += 1;
    }
    let name_start = i;
    while i < b.len() && (is_name_byte(b[i]) || matches!(b[i], b'-' | b':' | b'.')) {
        i += 1;
    }
    out.push((name_start, i, Token::Keyword));
    // Name of the attribute whose value may follow.
    let mut attribute: Option<(usize, usize)> = None;
    while i < b.len() && b[i] != b'>' {
        match b[i] {
            b'\'' | b'"' => {
                let end = string_end(src, i);
                let script = attribute
                    .is_some_and(|(s, e)| super::javascript::is_script_attribute(&src[s..e]));
                if script {
                    // Alpine / Livewire values are JavaScript: color the
                    // quotes as a string and the code inside as code.
                    let inner_end = end.saturating_sub(1).max(i + 1);
                    out.push((i, i + 1, Token::String));
                    super::javascript::scan_javascript(src, i + 1, inner_end, out);
                    if inner_end < end {
                        out.push((inner_end, end, Token::String));
                    }
                } else {
                    out.push((i, end, Token::String));
                }
                attribute = None;
                i = end;
            }
            b'{' if src[i..].starts_with("{{") => {
                out.push((i, i + 2, Token::Keyword));
                let (inner_end, end) = after(src, i + 2, "}}");
                scan_expression(src, i + 2, inner_end, out);
                if inner_end < end {
                    out.push((inner_end, end, Token::Keyword));
                }
                i = end;
            }
            b'@' => {
                // `@click="…"` is an Alpine event; `@if(…)`, `@class([…])`
                // are Blade directives.
                let start = i;
                let mut j = i + 1;
                while j < b.len() && (is_name_byte(b[j]) || matches!(b[j], b'-' | b':' | b'.')) {
                    j += 1;
                }
                if j > i + 1 && b.get(j) == Some(&b'=') {
                    out.push((start, j, Token::Function));
                    attribute = Some((start, j));
                    i = j;
                } else {
                    i = scan_directive(src, i, out);
                }
            }
            c if c.is_ascii_alphabetic() || c == b':' => {
                let start = i;
                while i < b.len() && (is_name_byte(b[i]) || matches!(b[i], b'-' | b':' | b'.')) {
                    i += 1;
                }
                let name = &src[start..i];
                let token = if super::javascript::is_script_attribute(name) {
                    Token::Function
                } else {
                    Token::Type
                };
                out.push((start, i, token));
                attribute = Some((start, i));
            }
            b'=' | b' ' | b'\n' | b'\t' | b'\r' => i += 1,
            _ => {
                attribute = None;
                i += 1;
            }
        }
    }
    (i + 1).min(b.len())
}

/// Scans a PHP expression in `start..end`: variables, strings, numbers,
/// member operators, calls, class names and keywords.
fn scan_expression(src: &str, start: usize, end: usize, out: &mut Vec<Span>) {
    let b = src.as_bytes();
    let mut i = start;
    while i < end {
        let c = b[i];
        if c == b'$'
            && b.get(i + 1)
                .is_some_and(|n| n.is_ascii_alphabetic() || *n == b'_')
        {
            let s = i;
            i += 1;
            while i < end && is_name_byte(b[i]) {
                i += 1;
            }
            out.push((s, i, Token::Variable));
        } else if c == b'\'' || c == b'"' {
            let e = string_end(src, i).min(end);
            out.push((i, e, Token::String));
            i = e;
        } else if c.is_ascii_digit() {
            let s = i;
            while i < end && (b[i].is_ascii_digit() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            out.push((s, i, Token::Number));
        } else if b[i..end].starts_with(b"?->") {
            out.push((i, i + 3, Token::Punct));
            i += 3;
        } else if b[i..end].starts_with(b"->")
            || b[i..end].starts_with(b"::")
            || b[i..end].starts_with(b"=>")
        {
            out.push((i, i + 2, Token::Punct));
            i += 2;
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'\\' {
            let s = i;
            while i < end && (is_name_byte(b[i]) || b[i] == b'\\') {
                i += 1;
            }
            let word = &src[s..i];
            let mut k = i;
            while k < end && b[k] == b' ' {
                k += 1;
            }
            let after_arrow = s >= 2 && (&b[s - 2..s] == b"->" || &b[s - 2..s] == b"::");
            let token = if KEYWORDS.contains(&word.to_ascii_lowercase().as_str()) && !after_arrow {
                Some(Token::Keyword)
            } else if b.get(k) == Some(&b'(') && k < end {
                Some(Token::Function)
            } else if word
                .trim_start_matches('\\')
                .starts_with(|c: char| c.is_ascii_uppercase())
                && !after_arrow
            {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(src: &str) -> Vec<(String, Token)> {
        let mut spans = Vec::new();
        scan_template(src, &mut spans);
        spans
            .into_iter()
            .map(|(s, e, t)| (src[s..e].to_string(), t))
            .collect()
    }

    #[test]
    fn highlights_blade_constructs() {
        let src = "{{-- note --}}\n<div class=\"x\" :id=\"$id\">@if ($user->isAdmin())\n{{ route('home') }} {!! $html !!}\n@endif mail@example.com @@if</div>";
        let t = tokens(src);
        let has = |s: &str, tok: Token| t.iter().any(|(x, k)| x == s && *k == tok);
        assert!(has("{{-- note --}}", Token::Comment));
        assert!(has("div", Token::Keyword));
        assert!(has("class", Token::Type));
        assert!(has("\"x\"", Token::String));
        assert!(has("@if", Token::Keyword));
        assert!(has("$user", Token::Variable));
        assert!(has("->", Token::Punct));
        assert!(has("isAdmin", Token::Function));
        assert!(has("route", Token::Function));
        assert!(has("'home'", Token::String));
        assert!(has("{!!", Token::Keyword));
        assert!(has("@endif", Token::Keyword));
        assert!(!t.iter().any(|(x, _)| x == "@example"));
        assert!(!t.iter().any(|(x, _)| x.contains("@@")));
    }

    #[test]
    fn compute_blade_highlights_should_split_lines() {
        let lines = compute_blade_highlights("{{--a\nb--}}");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1], vec![(0..5, Token::Comment)]);
    }
}
