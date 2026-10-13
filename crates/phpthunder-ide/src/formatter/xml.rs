//! Built-in XML formatter (`phpunit.xml`, `*.xml.dist`, `.svg`): one element
//! per line, indented by depth, without touching what is inside tags,
//! comments, CDATA or text. Elements holding only text stay on one line;
//! attributes continued on later lines are aligned under the first one.

/// One piece of an XML document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `<?xml … ?>`, `<!DOCTYPE …>`, `<!-- … -->`, `<![CDATA[ … ]]>`.
    Other,
    /// `<name …>`.
    Open,
    /// `</name>`.
    Close,
    /// `<name …/>`.
    Empty,
    /// Character data between tags (trimmed, never empty).
    Text,
}

/// A piece with its source text and the line breaks before it.
struct Piece<'a> {
    /// What it is.
    kind: Kind,
    /// Exact source text (text pieces trimmed).
    text: &'a str,
    /// Line breaks in the whitespace before it.
    breaks: usize,
}

/// Splits `src` into pieces.
fn lex(src: &str) -> Result<Vec<Piece<'_>>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut breaks = 0;
    while i < src.len() {
        let rest = &src[i..];
        if !rest.starts_with('<') {
            let end = rest.find('<').unwrap_or(rest.len());
            let chunk = &rest[..end];
            let text = chunk.trim();
            if text.is_empty() {
                breaks += chunk.matches('\n').count();
            } else {
                let lead = chunk.len() - chunk.trim_start().len();
                out.push(Piece {
                    kind: Kind::Text,
                    text,
                    breaks: breaks + chunk[..lead].matches('\n').count(),
                });
                breaks = chunk[lead + text.len()..].matches('\n').count();
            }
            i += end;
            continue;
        }
        let (kind, close) = if rest.starts_with("<!--") {
            (Kind::Other, "-->")
        } else if rest.starts_with("<![CDATA[") {
            (Kind::Other, "]]>")
        } else if rest.starts_with("<?") {
            (Kind::Other, "?>")
        } else if rest.starts_with("<!") || rest.starts_with("</") {
            (
                if rest.starts_with("</") {
                    Kind::Close
                } else {
                    Kind::Other
                },
                ">",
            )
        } else {
            (Kind::Open, ">")
        };
        let end = if kind == Kind::Open {
            tag_end(rest).ok_or("unterminated tag")?
        } else {
            rest.find(close).ok_or("unterminated markup")? + close.len()
        };
        let text = &rest[..end];
        let kind = if kind == Kind::Open && text.ends_with("/>") {
            Kind::Empty
        } else {
            kind
        };
        out.push(Piece { kind, text, breaks });
        breaks = 0;
        i += end;
    }
    Ok(out)
}

/// Length of the start tag at the beginning of `s`, skipping `>` inside
/// quoted attribute values.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// Element name of a start or end tag (`<a b="c">` and `</a>` give `a`).
fn tag_name(tag: &str) -> &str {
    tag.trim_start_matches(['<', '/'])
        .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .next()
        .unwrap_or_default()
}

/// Writes a tag at `depth`, re-indenting continuation lines of a
/// multi-line tag under its first attribute.
fn push_tag(out: &mut String, text: &str, pad: &str) {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default().trim_end();
    out.push_str(pad);
    out.push_str(first);
    let align = first.find(char::is_whitespace).map_or(2, |n| n + 1);
    for line in lines {
        out.push('\n');
        out.push_str(pad);
        // A lone `>` / `/>` closes the tag at the tag's own column.
        if !matches!(line.trim(), ">" | "/>") {
            out.push_str(&" ".repeat(align));
        }
        out.push_str(line.trim());
    }
}

/// Re-indents `src` with `indent` per level, keeping at most one blank line
/// between siblings. Ends with a single `\n`.
///
/// # Errors
/// Returns a message for unterminated tags or mismatched nesting, so the
/// caller leaves the file alone.
pub fn format(src: &str, indent: &str) -> Result<String, String> {
    let pieces = lex(src)?;
    let mut out = String::with_capacity(src.len());
    let mut open: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < pieces.len() {
        let p = &pieces[i];
        if !out.is_empty() {
            out.push('\n');
            if p.breaks > 1 && p.kind != Kind::Close {
                out.push('\n');
            }
        }
        if p.kind == Kind::Close {
            let name = tag_name(p.text);
            if open.pop() != Some(name) {
                return Err(format!("unexpected </{name}>"));
            }
        }
        let pad = indent.repeat(open.len());
        // `<a>text</a>` and `<a></a>` stay on one line.
        let inline_text = p.kind == Kind::Open
            && pieces
                .get(i + 1)
                .is_some_and(|t| t.kind == Kind::Text && !t.text.contains('\n'))
            && pieces.get(i + 2).is_some_and(|c| c.kind == Kind::Close);
        let inline_empty = p.kind == Kind::Open
            && pieces
                .get(i + 1)
                .is_some_and(|c| c.kind == Kind::Close && c.breaks == 0);
        match p.kind {
            Kind::Text => {
                for (n, line) in p.text.lines().enumerate() {
                    if n > 0 {
                        out.push('\n');
                    }
                    out.push_str(&pad);
                    out.push_str(line.trim());
                }
            }
            _ if (inline_text || inline_empty)
                && tag_name(pieces[i + if inline_text { 2 } else { 1 }].text)
                    != tag_name(p.text) =>
            {
                return Err(format!("unexpected </{}>", tag_name(pieces[i + 1].text)));
            }
            _ if inline_text => {
                push_tag(&mut out, p.text, &pad);
                out.push_str(pieces[i + 1].text);
                out.push_str(pieces[i + 2].text);
                i += 3;
                continue;
            }
            _ if inline_empty => {
                push_tag(&mut out, p.text, &pad);
                out.push_str(pieces[i + 1].text);
                i += 2;
                continue;
            }
            _ => push_tag(&mut out, p.text, &pad),
        }
        if p.kind == Kind::Open {
            open.push(tag_name(p.text));
        }
        i += 1;
    }
    if let Some(name) = open.last() {
        return Err(format!("unclosed <{name}>"));
    }
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_should_indent_elements_and_keep_text_inline() {
        let src = "<?xml version=\"1.0\"?>\n<phpunit bootstrap=\"vendor/autoload.php\"\n colors=\"true\">\n<testsuites><testsuite name=\"Unit\"><directory>tests/Unit</directory></testsuite></testsuites>\n\n\n<php><env name=\"APP_ENV\" value=\"testing\"/></php></phpunit>";
        assert_eq!(
            format(src, "    ").unwrap(),
            "<?xml version=\"1.0\"?>\n<phpunit bootstrap=\"vendor/autoload.php\"\n         colors=\"true\">\n    <testsuites>\n        <testsuite name=\"Unit\">\n            <directory>tests/Unit</directory>\n        </testsuite>\n    </testsuites>\n\n    <php>\n        <env name=\"APP_ENV\" value=\"testing\"/>\n    </php>\n</phpunit>\n"
        );
    }

    #[test]
    fn format_should_put_a_lone_closing_bracket_at_the_tag_column() {
        let src = "<phpunit a=\"1\"\n   b=\"2\"\n>\n<php/>\n</phpunit>";
        assert_eq!(
            format(src, "    ").unwrap(),
            "<phpunit a=\"1\"\n         b=\"2\"\n>\n    <php/>\n</phpunit>\n"
        );
    }

    #[test]
    fn format_should_keep_comments_and_cdata_verbatim() {
        let src = "<a><!-- keep  this --><b><![CDATA[ x < y ]]></b></a>";
        assert_eq!(
            format(src, "  ").unwrap(),
            "<a>\n  <!-- keep  this -->\n  <b>\n    <![CDATA[ x < y ]]>\n  </b>\n</a>\n"
        );
    }

    #[test]
    fn format_should_reject_broken_nesting() {
        assert!(format("<a><b></a>", "  ").is_err());
        assert!(format("<a><b></c></a>", "  ").is_err());
        assert!(format("<a>", "  ").is_err());
        assert!(format("</a>", "  ").is_err());
    }
}
