//! Whitespace clean-up every reformat ends with, and the whole reformat for
//! files without a formatter: trailing spaces, line endings and the final
//! newline, as the file's EditorConfig style asks.

use super::editorconfig::{EndOfLine, Style};

/// Normalizes `text` per `style`: strips trailing whitespace (unless
/// `trim_trailing_whitespace = false`; with `keep_hard_breaks`, a Markdown
/// line ending in two or more spaces keeps exactly two), converts line
/// endings to `end_of_line` (LF by default), and ends the text with exactly
/// one line break unless `insert_final_newline = false`.
pub fn normalize(text: &str, style: &Style, keep_hard_breaks: bool) -> String {
    let eol = style.end_of_line.unwrap_or(EndOfLine::Lf).as_str();
    let trim = style.trim_trailing_whitespace.unwrap_or(true);
    let lines: Vec<String> = text
        .split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if !trim {
                return line.to_string();
            }
            let trimmed = line.trim_end_matches([' ', '\t']);
            let trailing = line.len() - trimmed.len();
            if keep_hard_breaks && trailing >= 2 && !trimmed.is_empty() && line.ends_with("  ") {
                format!("{trimmed}  ")
            } else {
                trimmed.to_string()
            }
        })
        .collect();
    let mut out = lines.join(eol);
    if style.insert_final_newline.unwrap_or(true) {
        while out.ends_with(eol) {
            out.truncate(out.len() - eol.len());
        }
        if !out.is_empty() {
            out.push_str(eol);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_should_trim_and_end_with_one_newline() {
        let got = normalize("a  \r\nb\t\n\n\n", &Style::default(), false);
        assert_eq!(got, "a\nb\n");
    }

    #[test]
    fn normalize_should_keep_markdown_hard_breaks() {
        let got = normalize("line one   \nline two \n", &Style::default(), true);
        assert_eq!(got, "line one  \nline two\n");
    }

    #[test]
    fn normalize_should_follow_the_style() {
        let style = Style {
            end_of_line: Some(EndOfLine::CrLf),
            trim_trailing_whitespace: Some(false),
            insert_final_newline: Some(false),
            ..Style::default()
        };
        assert_eq!(normalize("a \nb", &style, false), "a \r\nb");
    }
}
