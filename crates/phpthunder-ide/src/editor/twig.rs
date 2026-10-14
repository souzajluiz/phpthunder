//! Twig template highlighting (Symfony, Drupal, Craft): `{# comments #}`,
//! `{{ output }}` and `{% tags %}` with the expressions inside (tag names,
//! operators, filters, strings, numbers, variables), over the HTML tags,
//! attributes and scripts the Blade scanner already reads.

use super::*;

/// Words colored as keywords inside Twig expressions.
const KEYWORDS: &[&str] = &[
    "and",
    "or",
    "not",
    "in",
    "is",
    "as",
    "with",
    "only",
    "ignore",
    "missing",
    "true",
    "false",
    "null",
    "none",
    "b-and",
    "b-or",
    "b-xor",
    "starts",
    "ends",
    "matches",
    "same",
    "defined",
    "empty",
    "iterable",
    "even",
    "odd",
    "divisible",
    "by",
    "from",
    "import",
    "if",
    "else",
    "elseif",
];

/// A scanned span: byte range of the document and its token.
type Span = (usize, usize, Token);

/// Highlight spans for a whole Twig template.
pub fn compute_twig_highlights(src: &str) -> LineSpans {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(src.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let mut lines: LineSpans = vec![Vec::new(); starts.len()];
    let mut spans = Vec::new();
    scan_twig(src, &mut spans);
    for (start, end, token) in spans {
        push_span(&mut lines, &starts, src, start, end, token);
    }
    lines
}

/// Offset of the first `needle` at or after `from` and the offset after
/// it; both the end of `src` when missing.
fn find(src: &str, from: usize, needle: &str) -> (usize, usize) {
    match src[from..].find(needle) {
        Some(i) => (from + i, from + i + needle.len()),
        None => (src.len(), src.len()),
    }
}

/// Scans the template: Twig blocks, HTML comments, then HTML tags (and the
/// JavaScript of `<script>` blocks) through the Blade scanner's tag reader.
fn scan_twig(src: &str, out: &mut Vec<Span>) {
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let rest = &b[i..];
        if rest.starts_with(b"{#") {
            let (_, end) = find(src, i + 2, "#}");
            out.push((i, end, Token::Comment));
            i = end;
        } else if rest.starts_with(b"{{") || rest.starts_with(b"{%") {
            let tag = rest[1] == b'%';
            let close = if tag { "%}" } else { "}}" };
            let (inner_end, end) = find(src, i + 2, close);
            out.push((i, i + 2, Token::Keyword));
            scan_expression(src, i + 2, inner_end, tag, out);
            if inner_end < end {
                out.push((inner_end, end, Token::Keyword));
            }
            i = end;
        } else if rest.starts_with(b"<!--") {
            let (_, end) = find(src, i + 4, "-->");
            out.push((i, end, Token::Comment));
            i = end;
        } else if b[i] == b'<'
            && b.get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/')
        {
            let tag_end = next_tag_end(src, i);
            let mut tag_spans = Vec::new();
            super::blade::scan_template(&src[i..tag_end], &mut tag_spans);
            out.extend(tag_spans.into_iter().map(|(s, e, t)| (s + i, e + i, t)));
            i = tag_end;
        } else {
            i += 1;
        }
    }
}

/// End of the HTML tag (or `<script>` block) starting at `at`, stopping at
/// the first Twig delimiter so Twig inside attributes keeps its colors.
fn next_tag_end(src: &str, at: usize) -> usize {
    let b = src.as_bytes();
    let is_script = src
        .get(at + 1..at + 7)
        .is_some_and(|t| t.eq_ignore_ascii_case("script"));
    if is_script {
        let close = src[at..].find("</script").map_or(src.len(), |n| at + n);
        return close;
    }
    let mut i = at + 1;
    while i < b.len() && b[i] != b'>' {
        if b[i] == b'{' && matches!(b.get(i + 1), Some(b'{' | b'%' | b'#')) {
            return i;
        }
        i += 1;
    }
    (i + 1).min(b.len())
}

/// Scans a Twig expression in `start..end`. In a `{% %}` block (`tag`) the
/// first word is the tag name; `|name` is a filter, `name(` a function,
/// `.name` an attribute.
fn scan_expression(src: &str, start: usize, end: usize, tag: bool, out: &mut Vec<Span>) {
    let b = src.as_bytes();
    let mut i = start;
    let mut first_word = tag;
    while i < end {
        let c = b[i];
        if c == b'\'' || c == b'"' {
            let mut e = i + 1;
            while e < end && b[e] != c {
                e += if b[e] == b'\\' { 2 } else { 1 };
            }
            let e = (e + 1).min(end);
            out.push((i, e, Token::String));
            i = e;
        } else if c.is_ascii_digit() {
            let s = i;
            while i < end && (b[i].is_ascii_digit() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            out.push((s, i, Token::Number));
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let s = i;
            while i < end && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
                i += 1;
            }
            // A trailing `-` is the whitespace-control marker, not the name.
            while i > s + 1 && b[i - 1] == b'-' {
                i -= 1;
            }
            let word = &src[s..i];
            let before = src[start..s].trim_end().as_bytes().last().copied();
            let token = if first_word {
                Token::Keyword
            } else if before == Some(b'|') || b.get(i) == Some(&b'(') {
                Token::Function
            } else if before == Some(b'.') {
                Token::Type
            } else if KEYWORDS.contains(&word) || word.starts_with("end") && tag && s == start {
                Token::Keyword
            } else {
                Token::Variable
            };
            first_word = false;
            out.push((s, i, token));
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `text` is highlighted as `token` in `src`.
    fn has(src: &str, text: &str, token: Token) -> bool {
        let lines = compute_twig_highlights(src);
        src.split('\n').zip(&lines).any(|(line, spans)| {
            spans
                .iter()
                .any(|(r, t)| &line[r.clone()] == text && *t == token)
        })
    }

    const SRC: &str = "{# note #}\n<a href=\"{{ path('home') }}\">{% for user in users|sort %}\n{{ user.name|upper }} {{ 42 }}{% endfor %}</a>";

    #[test]
    fn comments_and_tags_should_be_marked() {
        assert!(has(SRC, "{# note #}", Token::Comment));
        assert!(has(SRC, "for", Token::Keyword));
        assert!(has(SRC, "endfor", Token::Keyword));
        assert!(has(SRC, "in", Token::Keyword));
    }

    #[test]
    fn filters_functions_and_attributes_should_be_told_apart() {
        assert!(has(SRC, "sort", Token::Function));
        assert!(has(SRC, "path", Token::Function));
        assert!(has(SRC, "name", Token::Type));
        assert!(has(SRC, "user", Token::Variable));
        assert!(has(SRC, "'home'", Token::String));
        assert!(has(SRC, "42", Token::Number));
    }

    #[test]
    fn html_tags_should_keep_their_colors() {
        assert!(has(SRC, "a", Token::Keyword));
    }
}
