//! Copyright headers: the project's notice (from `.phpthunder/project.toml`)
//! written as a comment at the top of a file, replacing an older notice so
//! the year stays current. Used by the Update Copyright commands.

use crate::filetype::FileType;

/// How a file type writes the header comment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    /// `/* … */`, with ` * ` before each line.
    Block,
    /// `# ` before each line.
    Hash,
}

/// The comment style of `file_type`; `None` for types that get no header
/// (data files, Markdown, templates).
fn style(file_type: FileType) -> Option<Style> {
    match file_type {
        FileType::Php
        | FileType::JavaScript
        | FileType::TypeScript
        | FileType::Css
        | FileType::Scss => Some(Style::Block),
        FileType::Shell | FileType::Yaml | FileType::Dockerfile | FileType::Makefile => {
            Some(Style::Hash)
        }
        _ => None,
    }
}

/// Fills the notice's variables: `$year` (this year), `$project` (the
/// project folder's name) and `$file` (the file name).
pub fn render(template: &str, year: i32, project: &str, file: &str) -> String {
    template
        .trim()
        .replace("$year", &year.to_string())
        .replace("$project", project)
        .replace("$file", file)
}

/// The current year in UTC, from the system clock.
pub fn current_year() -> i32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Days since 1970-01-01 to a civil year (Howard Hinnant's algorithm).
    let days = (secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2)) as i32
}

/// `notice` as a comment in `style`, ending with a blank line.
fn comment(notice: &str, style: Style) -> String {
    let mut out = String::new();
    match style {
        Style::Block => {
            out.push_str("/*\n");
            for line in notice.lines() {
                out.push_str(if line.is_empty() { " *" } else { " * " });
                out.push_str(line);
                out.push('\n');
            }
            out.push_str(" */\n\n");
        }
        Style::Hash => {
            for line in notice.lines() {
                out.push_str(if line.is_empty() { "#" } else { "# " });
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
    }
    out
}

/// Byte offset where the header goes: after `<?php` in PHP, after a `#!`
/// line in scripts, else the start.
fn header_start(src: &str, file_type: FileType) -> usize {
    let first_line_end = |src: &str| src.find('\n').map_or(src.len(), |i| i + 1);
    if file_type == FileType::Php {
        let Some(open) = src.find("<?php") else {
            return 0;
        };
        let after = open + "<?php".len();
        let rest = &src[after..];
        let blank = rest.len() - rest.trim_start_matches([' ', '\t', '\r', '\n']).len();
        return after + blank;
    }
    if src.starts_with("#!") {
        return first_line_end(src);
    }
    0
}

/// Byte range of the comment right at `at` (skipping blank lines) when it
/// mentions "copyright": the old notice to replace, blank lines after it
/// included.
fn old_notice(src: &str, at: usize, style: Style) -> Option<(usize, usize)> {
    let rest = &src[at..];
    let start = at + (rest.len() - rest.trim_start_matches(['\r', '\n']).len());
    let body = &src[start..];
    let len = match style {
        Style::Block => {
            if !body.starts_with("/*") {
                return None;
            }
            body.find("*/")? + 2
        }
        Style::Hash => body
            .split_inclusive('\n')
            .take_while(|l| l.starts_with('#') && !l.starts_with("#!"))
            .map(str::len)
            .sum(),
    };
    if len == 0 || !body[..len].to_ascii_lowercase().contains("copyright") {
        return None;
    }
    let tail = &body[len..];
    let blank = tail.len() - tail.trim_start_matches(['\r', '\n', ' ', '\t']).len();
    Some((start, start + len + blank))
}

/// `src` with `notice` as its header: an older notice (a top comment that
/// mentions "copyright") is replaced, otherwise one is inserted. `None`
/// when the type gets no header or the text would not change.
pub fn apply(src: &str, file_type: FileType, notice: &str) -> Option<String> {
    let style = style(file_type)?;
    let notice = notice.trim();
    if notice.is_empty() {
        return None;
    }
    let at = header_start(src, file_type);
    let header = comment(notice, style);
    let (start, end) = old_notice(src, at, style).unwrap_or((at, at));
    let mut out = String::with_capacity(src.len() + header.len());
    out.push_str(&src[..start]);
    if file_type == FileType::Php && start > 0 && !out.ends_with("\n\n") {
        // `<?php` stays on its own line, followed by one blank line.
        out.truncate(out.trim_end().len());
        out.push_str("\n\n");
    }
    out.push_str(&header);
    out.push_str(&src[end..]);
    (out != src).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_should_fill_the_variables() {
        assert_eq!(
            render("(c) $year $project - $file\n", 2026, "shop", "a.php"),
            "(c) 2026 shop - a.php"
        );
    }

    #[test]
    fn php_header_should_follow_the_open_tag() {
        let out = apply(
            "<?php\nnamespace App;\n",
            FileType::Php,
            "Copyright 2026 Acme",
        );
        assert_eq!(
            out.as_deref(),
            Some("<?php\n\n/*\n * Copyright 2026 Acme\n */\n\nnamespace App;\n")
        );
    }

    #[test]
    fn old_notice_should_be_replaced() {
        let src = "<?php\n\n/*\n * Copyright 2020 Acme\n */\n\nnamespace App;\n";
        let out = apply(src, FileType::Php, "Copyright 2026 Acme").unwrap();
        assert_eq!(out, src.replace("2020", "2026"));
    }

    #[test]
    fn current_notice_should_change_nothing() {
        let src = "<?php\n\n/*\n * Copyright 2026 Acme\n */\n\nnamespace App;\n";
        assert_eq!(apply(src, FileType::Php, "Copyright 2026 Acme"), None);
    }

    #[test]
    fn doc_comments_without_copyright_should_be_kept() {
        let src = "<?php\n\n/** Helpers. */\nfunction a() {}\n";
        let out = apply(src, FileType::Php, "Copyright Acme").unwrap();
        assert!(out.ends_with("*/\n\n/** Helpers. */\nfunction a() {}\n"));
    }

    #[test]
    fn scripts_should_keep_the_shebang_first() {
        let out = apply("#!/bin/sh\necho hi\n", FileType::Shell, "Copyright Acme");
        assert_eq!(
            out.as_deref(),
            Some("#!/bin/sh\n# Copyright Acme\n\necho hi\n")
        );
    }

    #[test]
    fn data_files_should_get_no_header() {
        assert_eq!(apply("{}", FileType::Json, "Copyright Acme"), None);
    }

    #[test]
    fn current_year_should_be_plausible() {
        assert!((2024..2200).contains(&current_year()));
    }
}
