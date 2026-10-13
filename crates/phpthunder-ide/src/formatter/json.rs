//! Built-in JSON formatter (also JSONC with `//` and `/* */` comments).
//! Re-indents token by token instead of parsing into values, so key order,
//! number spelling, string escapes and comments survive untouched. Like the
//! usual JSON code style it removes blank lines and trailing commas.

/// One lexical piece of the input.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token<'a> {
    /// `{` or `[`.
    Open(u8),
    /// `}` or `]`.
    Close(u8),
    /// `,`.
    Comma,
    /// `:`.
    Colon,
    /// A comment, `//` or `/* */`, as written.
    Comment(&'a str),
    /// A string, number, `true`, `false` or `null`, as written.
    Atom(&'a str),
}

/// Tokens with the number of line breaks before each one in the input.
fn lex(src: &str) -> Result<Vec<(Token<'_>, usize)>, String> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut breaks) = (0, 0);
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        let token = match c {
            b'\n' => {
                breaks += 1;
                i += 1;
                continue;
            }
            c if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'{' | b'[' => {
                i += 1;
                Token::Open(c)
            }
            b'}' | b']' => {
                i += 1;
                Token::Close(c)
            }
            b',' => {
                i += 1;
                Token::Comma
            }
            b':' => {
                i += 1;
                Token::Colon
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                if i >= bytes.len() {
                    return Err("unterminated string".into());
                }
                i += 1;
                Token::Atom(&src[start..i])
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                Token::Comment(src[start..i].trim_end())
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let end = src[i + 2..].find("*/").ok_or("unterminated comment")?;
                i += end + 4;
                Token::Comment(&src[start..i])
            }
            _ => {
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !b"{}[],:\"/".contains(&bytes[i])
                {
                    i += 1;
                }
                Token::Atom(&src[start..i])
            }
        };
        out.push((token, breaks));
        breaks = 0;
    }
    Ok(out)
}

/// Re-indents `src` with `indent` per level, `"key": value` spacing and one
/// element per line; an innermost container written on a single line
/// (`["a", "b"]`, `{"php": "^8.3"}`) stays on one line, with `, ` between
/// elements. The outermost container is always expanded. Blank lines and commas
/// before `}` / `]` are dropped. Comments stay where they were
/// (trailing ones on their line). Ends with a single `\n`.
///
/// # Errors
/// Returns a message for unbalanced brackets or unterminated strings and
/// comments; the caller then leaves the file as it is.
pub fn format(src: &str, indent: &str) -> Result<String, String> {
    let tokens = lex(src)?;
    let inline = inline_containers(&tokens);
    // How many single-line containers enclose the current token.
    let mut inline_depth = 0usize;
    let mut out = String::with_capacity(src.len());
    let mut stack: Vec<u8> = Vec::new();
    // After `:` the value stays on the key's line, even past a comment.
    let mut pending_value = false;
    // Line break before an element, or `, ` spacing on a single line.
    let newline = |out: &mut String, depth: usize, inline_depth: usize| {
        if inline_depth > 0 {
            if out.ends_with(',') {
                out.push(' ');
            }
            return;
        }
        while out.ends_with(' ') {
            out.pop();
        }
        out.push('\n');
        out.push_str(&indent.repeat(depth));
    };
    for (k, (token, breaks)) in tokens.iter().enumerate() {
        let prev = k.checked_sub(1).map(|p| &tokens[p].0);
        let first_in_container = matches!(prev, Some(Token::Open(_)));
        match token {
            Token::Open(c) => {
                if pending_value {
                    space_before_value(&mut out);
                } else if !out.is_empty() {
                    newline(&mut out, stack.len(), inline_depth);
                }
                pending_value = false;
                out.push(*c as char);
                stack.push(*c);
                if inline_depth > 0 || inline.contains(&k) {
                    inline_depth += 1;
                }
            }
            Token::Close(c) => {
                let open = stack.pop().ok_or("unbalanced brackets")?;
                if (open == b'{') != (*c == b'}') {
                    return Err("mismatched brackets".into());
                }
                if !first_in_container && inline_depth == 0 {
                    newline(&mut out, stack.len(), inline_depth);
                }
                out.push(*c as char);
                inline_depth = inline_depth.saturating_sub(1);
            }
            // A trailing comma (only comments until `}` / `]`) is dropped.
            Token::Comma => {
                let next = tokens[k + 1..]
                    .iter()
                    .find(|(t, _)| !matches!(t, Token::Comment(_)));
                if !matches!(next, Some((Token::Close(_), _))) {
                    out.push(',');
                }
            }
            Token::Colon => {
                out.push(':');
                pending_value = true;
            }
            Token::Comment(text) => {
                let trailing = (*breaks == 0 && prev.is_some()) || pending_value;
                if trailing {
                    space_before_value(&mut out);
                } else if !out.is_empty() {
                    newline(&mut out, stack.len(), inline_depth);
                }
                out.push_str(text);
            }
            Token::Atom(text) => {
                if pending_value {
                    space_before_value(&mut out);
                } else if !out.is_empty() {
                    newline(&mut out, stack.len(), inline_depth);
                }
                pending_value = false;
                out.push_str(text);
            }
        }
    }
    if !stack.is_empty() {
        return Err("unbalanced brackets".into());
    }
    out.push('\n');
    Ok(out)
}

/// Token indices of the `{` / `[` that stay on one line: innermost (no
/// container inside), not the outermost, and written on one line without
/// comments.
fn inline_containers(tokens: &[(Token, usize)]) -> std::collections::HashSet<usize> {
    let mut out = std::collections::HashSet::new();
    let mut open: Vec<(usize, bool)> = Vec::new();
    for (k, (token, breaks)) in tokens.iter().enumerate() {
        // Any break or comment inside makes every enclosing container multi-line.
        if *breaks > 0 || matches!(token, Token::Comment(_)) {
            for (_, single) in open.iter_mut() {
                *single = false;
            }
        }
        match token {
            Token::Open(_) => {
                // A container holding another one is expanded.
                if let Some((_, single)) = open.last_mut() {
                    *single = false;
                }
                // The outermost container is expanded too.
                open.push((k, !open.is_empty()));
            }
            Token::Close(_) => {
                if let Some((start, true)) = open.pop() {
                    // Empty containers need no special case.
                    if k > start + 1 {
                        out.insert(start);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// One space between `:` (or an inline comment) and what follows.
fn space_before_value(out: &mut String) {
    if !out.ends_with(' ') {
        out.push(' ');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_should_indent_one_element_per_line() {
        let got = format(
            r#"{"name":"acme/app","require":{"php":"^8.3"},"keywords":[],"n":[1,2]}"#,
            "    ",
        )
        .unwrap();
        assert_eq!(
            got,
            "{\n    \"name\": \"acme/app\",\n    \"require\": {\"php\": \"^8.3\"},\n    \"keywords\": [],\n    \"n\": [1, 2]\n}\n"
        );
    }

    #[test]
    fn format_should_keep_comments_and_drop_blank_lines() {
        let src = "{\n  // build\n  \"a\": 1, // one\n\n\n  \"b\": /* two */ 2\n}";
        assert_eq!(
            format(src, "  ").unwrap(),
            "{\n  // build\n  \"a\": 1, // one\n  \"b\": /* two */ 2\n}\n"
        );
    }

    #[test]
    fn format_should_keep_single_line_containers() {
        let src = "{\"keywords\": [\"laravel\",\"framework\"],\n\"a\": {\"b\": [1, {\"c\": 2}]}\n}";
        assert_eq!(
            format(src, "    ").unwrap(),
            "{\n    \"keywords\": [\"laravel\", \"framework\"],\n    \"a\": {\n        \"b\": [\n            1,\n            {\"c\": 2}\n        ]\n    }\n}\n"
        );
    }

    #[test]
    fn format_should_drop_trailing_commas() {
        assert_eq!(
            format("{\"a\": [1, 2,], \"b\": 3, // last\n}", "  ").unwrap(),
            "{\n  \"a\": [1, 2],\n  \"b\": 3 // last\n}\n"
        );
    }

    #[test]
    fn format_should_reject_broken_input() {
        assert!(format("{\"a\": [1, 2}", "  ").is_err());
        assert!(format("{\"a\": \"x", "  ").is_err());
        assert!(format("[1, 2", "  ").is_err());
    }
}
