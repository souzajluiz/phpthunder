//! EditorConfig (`.editorconfig`): the base layer of every reformat. The
//! files from the edited file's folder up to the project root (or the first
//! one marked `root = true`) are read, nearer files winning, and the
//! sections whose glob matches the file give its whitespace style.

use std::path::Path;

/// Indentation character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentStyle {
    /// Spaces, `indent_size` per level.
    Space,
    /// One tab per level.
    Tab,
}

/// Line terminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndOfLine {
    /// `\n`.
    Lf,
    /// `\r\n`.
    CrLf,
    /// `\r`.
    Cr,
}

impl EndOfLine {
    /// The terminator text.
    pub fn as_str(self) -> &'static str {
        match self {
            EndOfLine::Lf => "\n",
            EndOfLine::CrLf => "\r\n",
            EndOfLine::Cr => "\r",
        }
    }
}

/// Whitespace style of one file; `None` fields were not configured and
/// fall back to the formatter's own default for the language.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Style {
    /// `indent_style`.
    pub indent_style: Option<IndentStyle>,
    /// `indent_size` (`tab` means `tab_width`).
    pub indent_size: Option<usize>,
    /// `tab_width`.
    pub tab_width: Option<usize>,
    /// `end_of_line`.
    pub end_of_line: Option<EndOfLine>,
    /// `trim_trailing_whitespace`.
    pub trim_trailing_whitespace: Option<bool>,
    /// `insert_final_newline`.
    pub insert_final_newline: Option<bool>,
    /// `max_line_length` (`off` leaves it unset).
    pub max_line_length: Option<usize>,
}

impl Style {
    /// One indentation level, given the language's default width.
    pub fn indent_unit(&self, default_size: usize) -> String {
        match self.indent_style {
            Some(IndentStyle::Tab) => "\t".to_string(),
            _ => " ".repeat(self.indent_size.or(self.tab_width).unwrap_or(default_size)),
        }
    }

    /// Applies one `key = value` pair; unknown keys and values are ignored,
    /// `unset` clears the property.
    fn set(&mut self, key: &str, value: &str) {
        let value = value.trim().to_ascii_lowercase();
        let flag = || match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "indent_style" => {
                self.indent_style = match value.as_str() {
                    "space" => Some(IndentStyle::Space),
                    "tab" => Some(IndentStyle::Tab),
                    _ => None,
                }
            }
            "indent_size" => {
                self.indent_size = match value.as_str() {
                    "tab" => self.tab_width,
                    v => v.parse().ok(),
                }
            }
            "tab_width" => self.tab_width = value.parse().ok(),
            "end_of_line" => {
                self.end_of_line = match value.as_str() {
                    "lf" => Some(EndOfLine::Lf),
                    "crlf" => Some(EndOfLine::CrLf),
                    "cr" => Some(EndOfLine::Cr),
                    _ => None,
                }
            }
            "trim_trailing_whitespace" => self.trim_trailing_whitespace = flag(),
            "insert_final_newline" => self.insert_final_newline = flag(),
            "max_line_length" => self.max_line_length = value.parse().ok(),
            _ => {}
        }
    }
}

/// The style for `file`, reading `.editorconfig` files from its folder up
/// to `root` (inclusive). Unreadable files are skipped.
pub fn style_for(file: &Path, root: &Path) -> Style {
    let mut configs = Vec::new();
    let mut dir = file.parent();
    while let Some(d) = dir {
        if let Ok(src) = std::fs::read_to_string(d.join(".editorconfig")) {
            let is_root = parse(&src).0;
            configs.push((d.to_path_buf(), src));
            if is_root {
                break;
            }
        }
        if d == root {
            break;
        }
        dir = d.parent();
    }
    let mut style = Style::default();
    // Farthest first, so nearer files override.
    for (dir, src) in configs.iter().rev() {
        let Ok(rel) = file.strip_prefix(dir) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        apply(src, &rel, &mut style);
    }
    style
}

/// Applies every section of one `.editorconfig` source whose glob matches
/// `rel` (the file's path relative to that config's folder), in order.
pub fn apply(src: &str, rel: &str, style: &mut Style) {
    for (glob, pairs) in parse(src).1 {
        if matches(&glob, rel) {
            for (k, v) in pairs {
                style.set(&k, &v);
            }
        }
    }
}

/// Parsed file: whether it is marked `root = true`, and its sections as
/// (glob, key/value pairs).
type Parsed = (bool, Vec<(String, Vec<(String, String)>)>);

/// Parses an `.editorconfig` source. Comments start with `#` or `;`.
fn parse(src: &str) -> Parsed {
    let mut root = false;
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if let Some(glob) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sections.push((glob.to_string(), Vec::new()));
        } else if let Some((k, v)) = line.split_once('=') {
            match sections.last_mut() {
                Some((_, pairs)) => pairs.push((k.trim().to_string(), v.trim().to_string())),
                None if k.trim().eq_ignore_ascii_case("root") => {
                    root = v.trim().eq_ignore_ascii_case("true");
                }
                None => {}
            }
        }
    }
    (root, sections)
}

/// EditorConfig glob match: a glob without `/` matches the file name at
/// any depth; `*` stops at `/`, `**` does not, `?` is one character,
/// `{a,b}` alternatives and `[abc]` / `[!abc]` classes are supported.
fn matches(glob: &str, rel: &str) -> bool {
    let glob = glob.strip_prefix('/').unwrap_or(glob);
    if glob.contains('/') {
        glob_match(glob.as_bytes(), rel.as_bytes())
    } else {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        glob_match(glob.as_bytes(), name.as_bytes())
    }
}

/// Recursive matcher behind [`matches`].
fn glob_match(p: &[u8], s: &[u8]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(b'*') if p.get(1) == Some(&b'*') => {
            let rest = &p[2..];
            (0..=s.len()).any(|i| glob_match(rest, &s[i..]))
        }
        Some(b'*') => {
            let rest = &p[1..];
            (0..=s.len())
                .take_while(|&i| i == 0 || s[i - 1] != b'/')
                .any(|i| glob_match(rest, &s[i..]))
        }
        Some(b'?') => !s.is_empty() && s[0] != b'/' && glob_match(&p[1..], &s[1..]),
        Some(b'{') => {
            let Some(close) = p.iter().position(|&c| c == b'}') else {
                return !s.is_empty() && s[0] == b'{' && glob_match(&p[1..], &s[1..]);
            };
            let rest = &p[close + 1..];
            if let Some((lo, hi)) = number_range(&p[1..close]) {
                // `{n..m}`: any integer from n to m, as written in the name.
                return (1..=s.len()).any(|end| {
                    std::str::from_utf8(&s[..end])
                        .ok()
                        .and_then(|n| n.parse::<i64>().ok())
                        .is_some_and(|n| (lo..=hi).contains(&n))
                        && glob_match(rest, &s[end..])
                });
            }
            p[1..close].split(|&c| c == b',').any(|alt| {
                let mut pattern = alt.to_vec();
                pattern.extend_from_slice(rest);
                glob_match(&pattern, s)
            })
        }
        Some(b'[') => {
            let Some(close) = p.iter().position(|&c| c == b']') else {
                return !s.is_empty() && s[0] == b'[' && glob_match(&p[1..], &s[1..]);
            };
            let Some(&c) = s.first() else {
                return false;
            };
            let (negate, set) = match p.get(1) {
                Some(b'!') => (true, &p[2..close]),
                _ => (false, &p[1..close]),
            };
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == b'-' {
                    hit |= (set[i]..=set[i + 2]).contains(&c);
                    i += 3;
                } else {
                    hit |= set[i] == c;
                    i += 1;
                }
            }
            hit != negate && glob_match(&p[close + 1..], &s[1..])
        }
        // `\` makes the next character literal (`\*`, `\{`).
        Some(b'\\') if p.len() > 1 => s.first() == Some(&p[1]) && glob_match(&p[2..], &s[1..]),
        Some(&c) => s.first() == Some(&c) && glob_match(&p[1..], &s[1..]),
    }
}

/// The bounds of a `{n..m}` numeric range body (`n..m`), lower first.
fn number_range(body: &[u8]) -> Option<(i64, i64)> {
    let body = std::str::from_utf8(body).ok()?;
    let (lo, hi) = body.split_once("..")?;
    let (lo, hi) = (lo.parse::<i64>().ok()?, hi.parse::<i64>().ok()?);
    Some((lo.min(hi), lo.max(hi)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_should_accept_numbers_inside_a_range() {
        assert!(matches("file{1..12}.txt", "file10.txt"));
    }

    #[test]
    fn matches_should_reject_numbers_outside_a_range() {
        assert!(!matches("file{1..12}.txt", "file13.txt"));
    }

    #[test]
    fn matches_should_treat_escaped_star_literally() {
        assert!(!matches("a\\*b", "axb"));
    }

    const CONFIG: &str = "root = true\n\n[*]\nindent_style = space\nindent_size = 4\ninsert_final_newline = true\ntrim_trailing_whitespace = true\n\n[*.{yml,yaml,json}]\nindent_size = 2\n\n[*.md]\ntrim_trailing_whitespace = false\n\n[docker-compose.yml]\nindent_size = 4\n\n[src/**.php]\nend_of_line = crlf\n";

    fn style(rel: &str) -> Style {
        let mut s = Style::default();
        apply(CONFIG, rel, &mut s);
        s
    }

    #[test]
    fn sections_should_apply_in_order_with_later_ones_winning() {
        assert_eq!(style("composer.json").indent_size, Some(2));
        assert_eq!(style("config/app.php").indent_size, Some(4));
        assert_eq!(style("docker-compose.yml").indent_size, Some(4));
        assert_eq!(
            style("docs/README.md").trim_trailing_whitespace,
            Some(false)
        );
    }

    #[test]
    fn globs_with_a_slash_should_match_from_the_config_folder() {
        assert_eq!(
            style("src/Http/Kernel.php").end_of_line,
            Some(EndOfLine::CrLf)
        );
        assert_eq!(style("app/Http/Kernel.php").end_of_line, None);
    }

    #[test]
    fn glob_classes_and_wildcards_should_match() {
        assert!(matches("*.[ch]", "x.c"));
        assert!(!matches("*.[!ch]", "x.c"));
        assert!(matches("a?c.txt", "abc.txt"));
        assert!(!matches("lib/*.js", "lib/a/b.js"));
        assert!(matches("lib/**.js", "lib/a/b.js"));
    }
}
