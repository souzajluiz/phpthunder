//! Turning a template into text at a position in a buffer.

use super::*;

/// Text to insert for a template, replacing `start..end` of the buffer,
/// with the caret's final byte offset in the new buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    /// Start of the replaced range (the abbreviation, or the caret).
    pub start: usize,
    /// End of the replaced range.
    pub end: usize,
    /// Inserted text.
    pub text: String,
    /// Caret offset after the edit.
    pub caret: usize,
}

/// Leading whitespace of the line containing `offset`.
fn line_indent(text: &str, offset: usize) -> &str {
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[line_start..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// `template` rendered to replace `start..end`, with its later lines
/// indented like the line it lands on.
pub fn render(template: &Template, text: &str, start: usize, end: usize) -> Expansion {
    let indent = line_indent(text, start);
    let body = template.body.replace('\n', &format!("\n{indent}"));
    let caret_in_body = body.find(END).unwrap_or(body.len());
    Expansion {
        start,
        end,
        text: body.replacen(END, "", 1),
        caret: start + caret_in_body,
    }
}

/// The expansion of the abbreviation just before `offset`, if it names a
/// template. Abbreviations count only as whole words that are not part of
/// a variable, member access or name (`$fore`, `->if`, `Foo\fn`).
pub fn expand_at(text: &str, offset: usize) -> Option<Expansion> {
    let before = text.get(..offset)?;
    let start = before
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map_or(0, |i| i + 1);
    let word = &before[start..];
    let prefix = before[..start].trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
    if word.is_empty() || ["$", "->", "::", "\\"].iter().any(|p| prefix.ends_with(p)) {
        return None;
    }
    find_template(word).map(|t| render(t, text, start, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_at_should_indent_and_place_the_caret() {
        let text = "<?php\n    fore";
        let e = expand_at(text, text.len()).unwrap();
        assert_eq!(e.text, "foreach ( as $item) {\n    }");
        assert_eq!(e.start, 10);
        assert_eq!(e.caret, 10 + "foreach (".len());
    }

    #[test]
    fn expand_at_should_ignore_variables_members_and_unknown_words() {
        assert_eq!(expand_at("$fore", 5), None);
        assert_eq!(expand_at("$a->if", 6), None);
        assert_eq!(expand_at("x forex", 7), None);
    }

    #[test]
    fn every_template_should_mark_the_caret_and_have_a_description() {
        for t in TEMPLATES {
            assert!(t.body.contains(END), "{}", t.abbreviation);
            assert!(!t.description().is_empty(), "{}", t.abbreviation);
        }
    }
}
