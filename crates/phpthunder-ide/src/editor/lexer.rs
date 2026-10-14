//! Highlighting for every file type that is not PHP or Blade, without a
//! grammar per language: one scanner driven by a small per-language
//! [`Spec`] (comment markers, quotes, keywords, how keys are written), plus
//! a line-oriented pass for Markdown. HTML, XML and Vue reuse the Blade
//! scanner, which already understands tags and attributes.

use super::*;
use crate::filetype::FileType;

/// How a language writes the key of a key/value pair.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Keys {
    /// No keys.
    None,
    /// A string followed by `:` (JSON).
    QuotedColon,
    /// A word at the start of a line followed by `:` (YAML, CSS properties).
    WordColon,
    /// A word at the start of a line followed by `=` or `:` (`.env`, INI, TOML).
    WordAssign,
}

/// What the scanner needs to know about one language.
struct Spec {
    /// Line comment markers (`//`, `#`, `--`).
    line_comments: &'static [&'static str],
    /// A `#` comment starts only at the start of a line or after
    /// whitespace (shell, YAML: `$#`, `a#b` are not comments).
    hash_needs_space: bool,
    /// Block comment delimiters.
    block_comment: Option<(&'static str, &'static str)>,
    /// String quote characters.
    quotes: &'static [u8],
    /// Keywords.
    keywords: &'static [&'static str],
    /// Keywords match regardless of case (SQL, Dockerfile).
    keywords_ignore_case: bool,
    /// Keywords count only as the first word of a line (Dockerfile).
    keywords_at_line_start: bool,
    /// `$name`, `${name}` and `$(name)` are variables.
    dollar_variables: bool,
    /// A word followed by `(` is a function call.
    calls: bool,
    /// How keys are written.
    keys: Keys,
    /// `[section]` lines are headers (INI, TOML).
    sections: bool,
    /// CSS extras: `@rules` as keywords and `#hex` colors as numbers.
    css: bool,
}

/// A spec with nothing enabled, completed per language.
const PLAIN: Spec = Spec {
    line_comments: &[],
    hash_needs_space: false,
    block_comment: None,
    quotes: &[],
    keywords: &[],
    keywords_ignore_case: false,
    keywords_at_line_start: false,
    dollar_variables: false,
    calls: false,
    keys: Keys::None,
    sections: false,
    css: false,
};

/// JavaScript and TypeScript keywords.
const JS_KEYWORDS: &[&str] = &[
    "as",
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
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "get",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "of",
    "private",
    "protected",
    "public",
    "readonly",
    "return",
    "set",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// SQL keywords (matched ignoring case).
const SQL_KEYWORDS: &[&str] = &[
    "add",
    "all",
    "alter",
    "and",
    "as",
    "asc",
    "between",
    "by",
    "case",
    "create",
    "cross",
    "default",
    "delete",
    "desc",
    "distinct",
    "drop",
    "else",
    "end",
    "exists",
    "foreign",
    "from",
    "full",
    "group",
    "having",
    "if",
    "in",
    "index",
    "inner",
    "insert",
    "into",
    "is",
    "join",
    "key",
    "left",
    "like",
    "limit",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "primary",
    "references",
    "right",
    "select",
    "set",
    "table",
    "then",
    "union",
    "unique",
    "update",
    "values",
    "view",
    "when",
    "where",
];

/// Shell keywords.
const SHELL_KEYWORDS: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "exit", "export", "fi", "for", "function", "if",
    "in", "local", "readonly", "return", "set", "then", "until", "while",
];

/// Dockerfile instructions (first word of a line).
const DOCKER_KEYWORDS: &[&str] = &[
    "add",
    "arg",
    "cmd",
    "copy",
    "entrypoint",
    "env",
    "expose",
    "from",
    "healthcheck",
    "label",
    "maintainer",
    "onbuild",
    "run",
    "shell",
    "stopsignal",
    "user",
    "volume",
    "workdir",
    "as",
];

/// Literal words of JSON, YAML, TOML and `.env` values.
const LITERALS: &[&str] = &["true", "false", "null", "yes", "no", "on", "off", "~"];

/// The scanner spec for `file_type`; `None` for types highlighted elsewhere
/// or not at all.
fn spec(file_type: FileType) -> Option<Spec> {
    Some(match file_type {
        FileType::JavaScript | FileType::TypeScript => Spec {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            quotes: b"'\"`",
            keywords: JS_KEYWORDS,
            calls: true,
            ..PLAIN
        },
        FileType::Json => Spec {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            quotes: b"\"",
            keywords: LITERALS,
            keys: Keys::QuotedColon,
            ..PLAIN
        },
        FileType::Yaml => Spec {
            line_comments: &["#"],
            hash_needs_space: true,
            quotes: b"'\"",
            keywords: LITERALS,
            keys: Keys::WordColon,
            ..PLAIN
        },
        FileType::Toml | FileType::Ini => Spec {
            line_comments: &["#", ";"],
            hash_needs_space: true,
            quotes: b"'\"",
            keywords: LITERALS,
            keys: Keys::WordAssign,
            sections: true,
            ..PLAIN
        },
        FileType::DotEnv => Spec {
            line_comments: &["#"],
            hash_needs_space: true,
            quotes: b"'\"",
            keywords: LITERALS,
            keys: Keys::WordAssign,
            dollar_variables: true,
            ..PLAIN
        },
        FileType::Css | FileType::Scss => Spec {
            line_comments: if file_type == FileType::Scss {
                &["//"]
            } else {
                &[]
            },
            block_comment: Some(("/*", "*/")),
            quotes: b"'\"",
            keys: Keys::WordColon,
            dollar_variables: file_type == FileType::Scss,
            calls: true,
            css: true,
            ..PLAIN
        },
        FileType::Sql => Spec {
            line_comments: &["--", "#"],
            block_comment: Some(("/*", "*/")),
            quotes: b"'\"`",
            keywords: SQL_KEYWORDS,
            keywords_ignore_case: true,
            calls: true,
            ..PLAIN
        },
        FileType::Shell => Spec {
            line_comments: &["#"],
            hash_needs_space: true,
            quotes: b"'\"",
            keywords: SHELL_KEYWORDS,
            dollar_variables: true,
            ..PLAIN
        },
        FileType::Dockerfile => Spec {
            line_comments: &["#"],
            hash_needs_space: true,
            quotes: b"'\"",
            keywords: DOCKER_KEYWORDS,
            keywords_ignore_case: true,
            keywords_at_line_start: true,
            dollar_variables: true,
            ..PLAIN
        },
        FileType::Makefile => Spec {
            line_comments: &["#"],
            hash_needs_space: true,
            quotes: b"'\"",
            dollar_variables: true,
            keys: Keys::WordColon,
            ..PLAIN
        },
        FileType::Ignore => Spec {
            line_comments: &["#"],
            hash_needs_space: true,
            ..PLAIN
        },
        _ => return None,
    })
}

/// Highlight spans for a file of `file_type` that is not PHP. HTML, XML
/// and Vue use the Blade scanner; Markdown its own pass; plain text, CSV
/// and unknown files get none.
pub fn compute_file_highlights(src: &str, file_type: FileType) -> LineSpans {
    match file_type {
        FileType::Blade | FileType::Html | FileType::Xml | FileType::Vue => {
            compute_blade_highlights(src)
        }
        FileType::Php => compute_highlights(src),
        FileType::Markdown => to_lines(src, markdown(src)),
        FileType::Log => to_lines(src, log(src)),
        FileType::Twig => compute_twig_highlights(src),
        other => match spec(other) {
            Some(spec) => to_lines(src, scan(src, &spec)),
            None => vec![Vec::new(); src.matches('\n').count() + 1],
        },
    }
}

/// A scanned span: byte range of the document and its token.
type Span = (usize, usize, Token);

/// A language written inside a PHP string literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Embedded {
    /// An SQL query (`"SELECT …"`, arguments of `whereRaw`, `DB::select`).
    Sql,
    /// An HTML fragment (`'<div class="x">'`).
    Html,
    /// A PCRE pattern (the first argument of `preg_*`).
    Regex,
}

/// Highlight spans of `snippet` (the content of a string literal) as
/// `language`, relative to the snippet.
///
/// Called by `compute_highlights` for string literals it recognizes.
pub fn embedded_spans(snippet: &str, language: Embedded) -> Vec<Span> {
    match language {
        Embedded::Sql => spec(FileType::Sql)
            .map(|s| scan(snippet, &s))
            .unwrap_or_default(),
        Embedded::Html => {
            let mut out = Vec::new();
            super::blade::scan_template(snippet, &mut out);
            out
        }
        Embedded::Regex => regex_spans(snippet),
    }
}

/// Spans of a PCRE pattern with its delimiters: delimiters and modifiers,
/// character classes, escapes, groups, quantifiers and anchors.
fn regex_spans(pattern: &str) -> Vec<Span> {
    let b = pattern.as_bytes();
    let mut out = Vec::new();
    let Some(&open) = b.first() else {
        return out;
    };
    let close = match open {
        b'(' => b')',
        b'{' => b'}',
        b'[' => b']',
        b'<' => b'>',
        other => other,
    };
    let Some(end) = b.iter().rposition(|&c| c == close).filter(|&e| e > 0) else {
        return out;
    };
    out.push((0, 1, Token::Keyword));
    out.push((end, b.len(), Token::Keyword));
    let mut i = 1;
    while i < end {
        match b[i] {
            b'\\' => {
                let next = (i + 2).min(end);
                out.push((i, next, Token::Number));
                i = next;
            }
            b'[' => {
                let start = i;
                i += 1;
                while i < end && b[i] != b']' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(end);
                out.push((start, i, Token::Type));
            }
            b'(' => {
                let start = i;
                i += 1;
                // `(?:`, `(?=`, `(?P<name>`: the group prefix is part of the paren.
                if b.get(i) == Some(&b'?') {
                    while i < end && !matches!(b[i], b':' | b'>' | b'=' | b'!' | b')') {
                        i += 1;
                    }
                    if i < end && b[i] != b')' {
                        i += 1;
                    }
                }
                out.push((start, i, Token::Function));
            }
            b')' => {
                out.push((i, i + 1, Token::Function));
                i += 1;
            }
            b'*' | b'+' | b'?' | b'|' => {
                out.push((i, i + 1, Token::Punct));
                i += 1;
            }
            b'{' => {
                let start = i;
                while i < end && b[i] != b'}' {
                    i += 1;
                }
                i = (i + 1).min(end);
                out.push((start, i, Token::Punct));
            }
            b'^' | b'$' | b'.' => {
                out.push((i, i + 1, Token::Keyword));
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// Splits document spans into per-line spans.
fn to_lines(src: &str, spans: Vec<Span>) -> LineSpans {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(src.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let mut lines: LineSpans = vec![Vec::new(); starts.len()];
    for (start, end, token) in spans {
        push_span(&mut lines, &starts, src, start, end, token);
    }
    lines
}

/// Whether `b` can be part of a word.
fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b >= 0x80
}

/// End of the string starting with the quote at `at` (escapes honoured);
/// unterminated strings end at the end of the line.
fn string_end(src: &str, at: usize) -> usize {
    let bytes = src.as_bytes();
    let quote = bytes[at];
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\n' if quote != b'`' => return i,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Whether only spaces and tabs come before `at` on its line.
fn at_line_start(src: &str, at: usize) -> bool {
    src[..at]
        .bytes()
        .rev()
        .take_while(|&b| b != b'\n')
        .all(|b| b == b' ' || b == b'\t' || b == b'-')
}

/// Byte after `at`, skipping spaces and tabs.
fn next_non_space(src: &str, at: usize) -> Option<u8> {
    src[at..].bytes().find(|&b| b != b' ' && b != b'\t')
}

/// Scans `src` with a language spec.
fn scan(src: &str, spec: &Spec) -> Vec<Span> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &src[i..];
        let c = bytes[i];
        // Comments.
        if let Some((open, close)) = spec.block_comment
            && rest.starts_with(open)
        {
            let end = rest[open.len()..]
                .find(close)
                .map_or(src.len(), |e| i + open.len() + e + close.len());
            out.push((i, end, Token::Comment));
            i = end;
            continue;
        }
        let line_comment = spec.line_comments.iter().any(|m| rest.starts_with(m))
            && (c != b'#'
                || !spec.hash_needs_space
                || i == 0
                || bytes[i - 1].is_ascii_whitespace());
        if line_comment && !(spec.css && c == b'#') {
            let end = rest.find('\n').map_or(src.len(), |e| i + e);
            out.push((i, end, Token::Comment));
            i = end;
            continue;
        }
        // `[section]` headers.
        if spec.sections && c == b'[' && at_line_start(src, i) {
            let end = rest.find('\n').map_or(src.len(), |e| i + e);
            let close = rest[..end - i].rfind(']').map_or(end, |e| i + e + 1);
            out.push((i, close, Token::Type));
            i = close;
            continue;
        }
        // Strings, or JSON keys.
        if spec.quotes.contains(&c) {
            let end = string_end(src, i);
            let is_key = spec.keys == Keys::QuotedColon && next_non_space(src, end) == Some(b':');
            out.push((
                i,
                end,
                if is_key {
                    Token::Variable
                } else {
                    Token::String
                },
            ));
            i = end;
            continue;
        }
        // Variables: `$name`, `${name}`, `$(name)`.
        if spec.dollar_variables && c == b'$' {
            let end = match bytes.get(i + 1) {
                Some(b'{') => rest.find('}').map_or(src.len(), |e| i + e + 1),
                Some(b'(') => rest.find(')').map_or(src.len(), |e| i + e + 1),
                _ => {
                    let len = rest[1..]
                        .bytes()
                        .take_while(|&b| is_word(b) && b != b'-')
                        .count();
                    i + 1 + len
                }
            };
            if end > i + 1 {
                out.push((i, end, Token::Variable));
                i = end;
                continue;
            }
        }
        // CSS `@media`, `#fff`.
        if spec.css && (c == b'@' || c == b'#') {
            let len = rest[1..].bytes().take_while(|&b| is_word(b)).count();
            if len > 0 {
                let token = if c == b'@' {
                    Token::Keyword
                } else {
                    Token::Number
                };
                out.push((i, i + 1 + len, token));
                i += 1 + len;
                continue;
            }
        }
        // Numbers.
        if c.is_ascii_digit() && (i == 0 || !is_word(bytes[i - 1])) {
            let len = rest
                .bytes()
                .take_while(|&b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'%')
                .count();
            out.push((i, i + len, Token::Number));
            i += len;
            continue;
        }
        // Words: keys, keywords, calls.
        if is_word(c) && c != b'-' {
            let len = rest
                .bytes()
                .take_while(|&b| is_word(b) || b == b'.')
                .count();
            let word = &rest[..len];
            let end = i + len;
            let after = next_non_space(src, end);
            let starts_line = at_line_start(src, i);
            let token = if starts_line
                && match spec.keys {
                    Keys::WordColon => {
                        after == Some(b':') && src[end..].trim_start().get(1..2) != Some(":")
                    }
                    Keys::WordAssign => after == Some(b'=') || after == Some(b':'),
                    _ => false,
                } {
                Some(Token::Variable)
            } else if (!spec.keywords_at_line_start || starts_line)
                && spec.keywords.iter().any(|k| {
                    if spec.keywords_ignore_case {
                        k.eq_ignore_ascii_case(word)
                    } else {
                        *k == word
                    }
                })
            {
                Some(Token::Keyword)
            } else if spec.calls && after == Some(b'(') {
                Some(Token::Function)
            } else {
                None
            };
            if let Some(token) = token {
                out.push((i, end, token));
            }
            i = end;
            continue;
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// Markdown: headings, fenced code blocks, inline code, link targets, list
/// and quote markers.
fn markdown(src: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut offset = 0;
    let mut fence: Option<&str> = None;
    for line in src.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        let trimmed = body.trim_start();
        let lead = body.len() - trimmed.len();
        if let Some(marker) = fence {
            out.push((start, start + body.len(), Token::String));
            if trimmed.starts_with(marker) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(&trimmed[..3]);
            out.push((start, start + body.len(), Token::String));
            continue;
        }
        if trimmed.starts_with('#') {
            out.push((start, start + body.len(), Token::Keyword));
            continue;
        }
        if trimmed.starts_with('>') {
            out.push((start, start + body.len(), Token::Comment));
            continue;
        }
        // `-`, `*`, `+` or `1.` / `1)` followed by a space.
        let is_marker = |m: &&str| {
            matches!(*m, "-" | "*" | "+")
                || (m.len() > 1
                    && m.ends_with(['.', ')'])
                    && m[..m.len() - 1].bytes().all(|b| b.is_ascii_digit()))
        };
        let marker = trimmed.split_once(' ').map(|(m, _)| m).filter(is_marker);
        if let Some(m) = marker {
            out.push((start + lead, start + lead + m.len(), Token::Punct));
        }
        // Inline code and link targets.
        let bytes = body.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'`' => {
                    let end = body[i + 1..].find('`').map_or(body.len(), |e| i + e + 2);
                    out.push((start + i, start + end, Token::String));
                    i = end;
                }
                b']' if bytes.get(i + 1) == Some(&b'(') => {
                    let end = body[i..].find(')').map_or(body.len(), |e| i + e + 1);
                    out.push((start + i + 1, start + end, Token::Function));
                    i = end;
                }
                _ => i += 1,
            }
        }
    }
    out
}

/// Log highlighting: stack-trace lines dimmed, then per line the
/// timestamp, the level (errors, warnings, info, debug in their own
/// colors) and quoted text. Monolog, PHP's error log, nginx and most
/// framework logs follow this shape.
fn log(src: &str) -> Vec<Span> {
    use std::sync::OnceLock;
    static PATTERNS: OnceLock<[regex::Regex; 3]> = OnceLock::new();
    let [stamp, level, quoted] = PATTERNS.get_or_init(|| {
        [
            regex::Regex::new(
                r"^\[?\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}[^\] ]*\]?|^\[\d{2}-\w{3}-\d{4} [^\]]+\]",
            )
            .expect("valid timestamp pattern"),
            regex::Regex::new(
                r"(?i)\b(EMERGENCY|ALERT|CRITICAL|ERROR|FATAL|SEVERE|WARNING|WARN|NOTICE|DEPRECATED|INFO|DEBUG|TRACE)\b",
            )
            .expect("valid level pattern"),
            regex::Regex::new(r#""[^"\n]*""#).expect("valid quote pattern"),
        ]
    });
    let mut out = Vec::new();
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        let trimmed = body.trim_start();
        let is_frame = trimmed.starts_with('#')
            && trimmed[1..].starts_with(|c: char| c.is_ascii_digit())
            || trimmed.starts_with("at ")
            || trimmed.starts_with("Stack trace:")
            || trimmed.starts_with("[stacktrace]");
        if is_frame {
            out.push((start, start + body.len(), Token::Comment));
            continue;
        }
        let mut from = 0;
        if let Some(m) = stamp.find(body) {
            out.push((start + m.start(), start + m.end(), Token::Number));
            from = m.end();
        }
        if let Some(m) = level.find_at(body, from) {
            let token = match m.as_str().to_ascii_uppercase().as_str() {
                "WARNING" | "WARN" | "NOTICE" | "DEPRECATED" => Token::Function,
                "INFO" => Token::String,
                "DEBUG" | "TRACE" => Token::Comment,
                _ => Token::Keyword,
            };
            out.push((start + m.start(), start + m.end(), token));
            from = m.end();
        }
        for m in quoted.find_iter(&body[from..]) {
            out.push((
                start + from + m.start(),
                start + from + m.end(),
                Token::String,
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The (text, token) pairs highlighted in `src`.
    fn tokens(src: &str, file_type: FileType) -> Vec<(String, Token)> {
        let lines = compute_file_highlights(src, file_type);
        src.split('\n')
            .zip(lines.iter())
            .flat_map(|(line, spans)| {
                spans
                    .iter()
                    .map(|(r, t)| (line[r.clone()].to_string(), *t))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Whether `text` is highlighted as `token`.
    fn has(src: &str, file_type: FileType, text: &str, token: Token) -> bool {
        tokens(src, file_type)
            .iter()
            .any(|(t, k)| t == text && *k == token)
    }

    #[test]
    fn log_should_mark_dates_levels_and_traces() {
        let src = "[2026-10-14 10:00:00] local.ERROR: Failed \"x\"\n#0 /app/a.php(3): f()\n[2026-10-14 10:00:01] local.INFO: ok\n";
        assert!(has(
            src,
            FileType::Log,
            "[2026-10-14 10:00:00]",
            Token::Number
        ));
        assert!(has(src, FileType::Log, "ERROR", Token::Keyword));
        assert!(has(src, FileType::Log, "\"x\"", Token::String));
        assert!(has(
            src,
            FileType::Log,
            "#0 /app/a.php(3): f()",
            Token::Comment
        ));
        assert!(has(src, FileType::Log, "INFO", Token::String));
    }

    #[test]
    fn json_should_tell_keys_from_values() {
        let src = "{\n  \"name\": \"acme\", // app\n  \"ok\": true, \"n\": 3\n}";
        assert!(has(src, FileType::Json, "\"name\"", Token::Variable));
        assert!(has(src, FileType::Json, "\"acme\"", Token::String));
        assert!(has(src, FileType::Json, "// app", Token::Comment));
        assert!(has(src, FileType::Json, "true", Token::Keyword));
        assert!(has(src, FileType::Json, "3", Token::Number));
    }

    #[test]
    fn yaml_env_and_ini_should_highlight_keys_and_comments() {
        let yaml = "services:\n  app:\n    image: 'php:8.3' # base\n    url: http://a#b\n";
        assert!(has(yaml, FileType::Yaml, "services", Token::Variable));
        assert!(has(yaml, FileType::Yaml, "image", Token::Variable));
        assert!(has(yaml, FileType::Yaml, "# base", Token::Comment));
        assert!(!tokens(yaml, FileType::Yaml).iter().any(|(t, _)| t == "#b"));
        let env = "# db\nDB_HOST=127.0.0.1\nAPP_URL=\"${HOST}/x\"";
        assert!(has(env, FileType::DotEnv, "DB_HOST", Token::Variable));
        assert!(has(env, FileType::DotEnv, "# db", Token::Comment));
        let ini = "[PHP]\nmemory_limit = 512M ; dev";
        assert!(has(ini, FileType::Ini, "[PHP]", Token::Type));
        assert!(has(ini, FileType::Ini, "memory_limit", Token::Variable));
        assert!(has(ini, FileType::Ini, "; dev", Token::Comment));
    }

    #[test]
    fn code_like_languages_should_highlight_keywords_calls_and_strings() {
        let js = "import x from 'y';\nconst a = fetch(`/api`); // go";
        assert!(has(js, FileType::JavaScript, "import", Token::Keyword));
        assert!(has(js, FileType::JavaScript, "fetch", Token::Function));
        assert!(has(js, FileType::JavaScript, "`/api`", Token::String));
        let sql = "SELECT id FROM users -- all\nWHERE id = 1;";
        assert!(has(sql, FileType::Sql, "SELECT", Token::Keyword));
        assert!(has(sql, FileType::Sql, "-- all", Token::Comment));
        let sh = "#!/bin/bash\nif [ -n \"$HOME\" ]; then echo ${USER}; fi";
        assert!(has(sh, FileType::Shell, "if", Token::Keyword));
        assert!(has(sh, FileType::Shell, "${USER}", Token::Variable));
        let docker = "FROM php:8.3-fpm AS base\nRUN apt-get update # deps";
        assert!(has(docker, FileType::Dockerfile, "FROM", Token::Keyword));
        assert!(has(docker, FileType::Dockerfile, "RUN", Token::Keyword));
        assert!(!has(docker, FileType::Dockerfile, "update", Token::Keyword));
    }

    #[test]
    fn css_should_highlight_properties_rules_and_colors() {
        let css =
            "@media (max-width: 600px) {\n  .a { color: #fff; }\n  body {\n    margin: 0;\n  }\n}";
        assert!(has(css, FileType::Css, "@media", Token::Keyword));
        assert!(has(css, FileType::Css, "#fff", Token::Number));
        assert!(has(css, FileType::Css, "margin", Token::Variable));
    }

    #[test]
    fn markdown_should_highlight_headings_code_and_links() {
        let md = "# Title\n- item with `code` and [link](https://x.y)\n```php\n$a = 1;\n```";
        assert!(has(md, FileType::Markdown, "# Title", Token::Keyword));
        assert!(has(md, FileType::Markdown, "`code`", Token::String));
        assert!(has(
            md,
            FileType::Markdown,
            "(https://x.y)",
            Token::Function
        ));
        assert!(has(md, FileType::Markdown, "$a = 1;", Token::String));
        assert!(has(md, FileType::Markdown, "-", Token::Punct));
    }

    #[test]
    fn every_type_should_survive_unicode_and_unterminated_input() {
        let nasty = "çã \"não fechou\n/* aberto ção\n`crase ${é\n# título ü\n[seção\nchave_ç = 'x\n$(\n@\n#\n\\é\n";
        for ft in [
            FileType::JavaScript,
            FileType::Json,
            FileType::Yaml,
            FileType::Toml,
            FileType::Ini,
            FileType::DotEnv,
            FileType::Css,
            FileType::Scss,
            FileType::Sql,
            FileType::Shell,
            FileType::Dockerfile,
            FileType::Makefile,
            FileType::Ignore,
            FileType::Markdown,
            FileType::Html,
            FileType::Twig,
            FileType::Log,
            FileType::Text,
        ] {
            let lines = compute_file_highlights(nasty, ft);
            for (line, spans) in nasty.split('\n').zip(&lines) {
                for (range, _) in spans {
                    assert!(
                        line.get(range.clone()).is_some(),
                        "{ft:?}: {range:?} in {line:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn plain_text_should_have_one_empty_line_per_line() {
        assert_eq!(compute_file_highlights("a\nb\n", FileType::Text).len(), 3);
    }
}
