//! Spell checking of PHP comments, string literals and declared names
//! (split at camel case and underscores), and of Markdown and plain text
//! files, with Hunspell dictionaries read by `spellbook`: English and
//! Brazilian Portuguese are bundled, other languages are the `.aff`/`.dic` pairs in
//! `~/.config/phpthunder/dictionaries`, and words the user saves go to
//! `~/.config/phpthunder/dictionary.txt`.

use crate::tr;
use parking_lot::RwLock;
use phpthunder::inspection::{Code, Diagnostic, Severity};
use phpthunder::syntax::{LineIndex, parse};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The bundled English dictionary (SCOWL, via wooorm/dictionaries).
const EN_AFF: &str = include_str!("../assets/dictionaries/en_US.aff");
/// Word list of the bundled English dictionary.
const EN_DIC: &str = include_str!("../assets/dictionaries/en_US.dic");
/// The bundled Brazilian Portuguese dictionary (VERO), gzipped.
const PT_AFF: &[u8] = include_bytes!("../assets/dictionaries/pt_BR.aff.gz");
/// Word list of the bundled Portuguese dictionary, gzipped.
const PT_DIC: &[u8] = include_bytes!("../assets/dictionaries/pt_BR.dic.gz");

/// Unpacks a gzipped bundled file.
fn gunzip(bytes: &[u8]) -> Option<String> {
    use std::io::Read;
    let mut out = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut out)
        .ok()?;
    Some(out)
}

/// Source code words no dictionary has: never typos.
const PROGRAMMING: &[&str] = &[
    "php",
    "html",
    "css",
    "json",
    "yaml",
    "xml",
    "sql",
    "http",
    "https",
    "url",
    "uri",
    "api",
    "uuid",
    "utf",
    "args",
    "argv",
    "param",
    "params",
    "bool",
    "int",
    "str",
    "num",
    "arr",
    "obj",
    "attr",
    "attrs",
    "config",
    "env",
    "dev",
    "prod",
    "repo",
    "auth",
    "admin",
    "async",
    "init",
    "impl",
    "lang",
    "len",
    "src",
    "dest",
    "dir",
    "dirs",
    "tmp",
    "temp",
    "cli",
    "cmd",
    "ctx",
    "db",
    "eloquent",
    "laravel",
    "livewire",
    "filament",
    "symfony",
    "phpunit",
    "pest",
    "composer",
    "namespace",
    "middleware",
    "enum",
    "enums",
    "nullable",
    "boolean",
    "datetime",
    "timestamp",
    "timestamps",
    "foreach",
    "elseif",
    "endif",
    "endforeach",
    "stdout",
    "stderr",
    "regex",
    "subquery",
    "upsert",
    "mixin",
    "mixins",
    "trait",
    "traits",
    "getter",
    "setter",
    "getters",
    "setters",
    "callable",
    "iterable",
    "readonly",
    "inline",
    "webhook",
    "webhooks",
    "todo",
    "fixme",
    "noop",
    "localhost",
    "idx",
    "msg",
    "req",
    "res",
    "resp",
    "btn",
    "img",
    "nav",
    "ajax",
    "csrf",
    "jwt",
    "oauth",
    "smtp",
    "mysql",
    "postgres",
    "sqlite",
    "redis",
    "docker",
    "github",
    "gitlab",
    "npm",
    "vite",
    "tailwind",
    "alpine",
    "vue",
    "typescript",
    "javascript",
    "phpdoc",
    "dto",
    "dtos",
    "vo",
];

/// The loaded dictionaries and the user's words.
struct Speller {
    /// Hunspell dictionaries: bundled English plus the user's.
    dictionaries: Vec<spellbook::Dictionary>,
    /// Words saved by the user, lowercase.
    learned: HashSet<String>,
}

/// `~/.config/phpthunder`.
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/phpthunder"))
}

/// The user's dictionary file.
fn learned_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("dictionary.txt"))
}

impl Speller {
    /// Loads the bundled dictionary, the user's dictionaries and words.
    fn load() -> Self {
        let mut dictionaries = Vec::new();
        match spellbook::Dictionary::new(EN_AFF, EN_DIC) {
            Ok(d) => dictionaries.push(d),
            Err(e) => crate::log::error(format_args!("bundled dictionary: {e}")),
        }
        if let (Some(aff), Some(dic)) = (gunzip(PT_AFF), gunzip(PT_DIC)) {
            match spellbook::Dictionary::new(&aff, &dic) {
                Ok(d) => dictionaries.push(d),
                Err(e) => crate::log::error(format_args!("bundled pt_BR dictionary: {e}")),
            }
        }
        let extra = config_dir().map(|d| d.join("dictionaries"));
        for entry in extra
            .and_then(|d| std::fs::read_dir(d).ok())
            .into_iter()
            .flatten()
            .flatten()
        {
            let dic = entry.path();
            if dic.extension().is_none_or(|e| e != "dic") {
                continue;
            }
            let (Ok(dic_text), Ok(aff_text)) = (
                std::fs::read_to_string(&dic),
                std::fs::read_to_string(dic.with_extension("aff")),
            ) else {
                continue;
            };
            match spellbook::Dictionary::new(&aff_text, &dic_text) {
                Ok(d) => dictionaries.push(d),
                Err(e) => crate::log::error(format_args!("{}: {e}", dic.display())),
            }
        }
        let learned = learned_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| {
                t.lines()
                    .map(|l| l.trim().to_lowercase())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            dictionaries,
            learned,
        }
    }

    /// Whether `word` is spelled right (or not worth checking).
    fn accepts(&self, word: &str) -> bool {
        let lower = word.to_lowercase();
        PROGRAMMING.contains(&lower.as_str())
            || self.learned.contains(&lower)
            || self.dictionaries.is_empty()
            || self
                .dictionaries
                .iter()
                .any(|d| d.check(word) || d.check(&lower))
    }
}

/// The shared speller, loaded on first use (off the UI thread: the first
/// inspection pays for parsing the dictionaries).
fn speller() -> &'static RwLock<Speller> {
    static SPELLER: OnceLock<RwLock<Speller>> = OnceLock::new();
    SPELLER.get_or_init(|| RwLock::new(Speller::load()))
}

/// Corrections for `word`, best first (at most five).
pub fn suggestions(word: &str) -> Vec<String> {
    let speller = speller().read();
    let mut out = Vec::new();
    for d in &speller.dictionaries {
        let mut found = Vec::new();
        d.suggest(word, &mut found);
        out.extend(found);
    }
    out.dedup();
    out.truncate(5);
    out
}

/// Saves `word` to the user's dictionary: it is never reported again.
pub fn learn(word: &str) {
    let lower = word.to_lowercase();
    speller().write().learned.insert(lower.clone());
    if let Some(path) = learned_path() {
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(&path));
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&lower);
        text.push('\n');
        let _ = std::fs::write(&path, text);
    }
}

/// Words of `text` (offset `base` in the file) worth checking: letter
/// runs split at camel-case humps, three letters or longer, not all
/// capitals (acronyms), not touching digits.
fn words(text: &str, base: usize, out: &mut Vec<(usize, usize)>) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].1.is_alphabetic() {
            i += 1;
            continue;
        }
        let run_start = i;
        while i < chars.len() && chars[i].1.is_alphanumeric() {
            i += 1;
        }
        let run = &chars[run_start..i];
        if run.iter().any(|(_, c)| c.is_ascii_digit()) {
            continue;
        }
        // Split the run at humps: `getUserName`, `HTTPRequest`.
        let mut start = 0;
        for k in 1..=run.len() {
            let boundary = k == run.len()
                || (run[k].1.is_uppercase() && run[k - 1].1.is_lowercase())
                || (k + 1 < run.len()
                    && run[k].1.is_uppercase()
                    && run[k - 1].1.is_uppercase()
                    && run[k + 1].1.is_lowercase());
            if boundary {
                let piece = &run[start..k];
                let all_caps = piece.iter().all(|(_, c)| c.is_uppercase());
                if piece.len() >= 3 && !all_caps {
                    let from = base + piece[0].0;
                    let last = piece[piece.len() - 1];
                    out.push((from, base + last.0 + last.1.len_utf8()));
                }
                start = k;
            }
        }
    }
}

/// Ranges of a PHP file to check: comments (doc tags and `$variables`
/// skipped), string contents, and the names of declarations.
fn php_ranges(src: &str) -> Vec<(usize, usize)> {
    let tree = parse(src);
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "comment" => {
                let text = &src[node.byte_range()];
                let mut offset = node.start_byte();
                for line in text.split_inclusive('\n') {
                    let checked = match line.find('@') {
                        // A doc tag line: only the description after the tag's
                        // type and variable.
                        Some(at) if line[..at].trim_start_matches([' ', '*', '/']).is_empty() => {
                            let after_var = line
                                .find('$')
                                .and_then(|d| line[d..].find(' ').map(|s| d + s));
                            after_var.map(|s| (s, line.len()))
                        }
                        _ => Some((0, line.len())),
                    };
                    if let Some((a, b)) = checked {
                        let piece = &line[a..b];
                        // URLs and paths are not prose.
                        if !piece.contains("://") {
                            out.push((offset + a, offset + b));
                        }
                    }
                    offset += line.len();
                }
                continue;
            }
            "string_content" => {
                let text = &src[node.byte_range()];
                // Keys, paths and class names are not prose.
                if text.contains(' ') && !text.contains("://") {
                    out.push((node.start_byte(), node.end_byte()));
                }
                continue;
            }
            "name"
                if node.parent().is_some_and(|p| {
                    matches!(
                        p.kind(),
                        "class_declaration"
                            | "interface_declaration"
                            | "trait_declaration"
                            | "enum_declaration"
                            | "function_definition"
                            | "method_declaration"
                            | "const_element"
                            | "enum_case"
                    )
                }) =>
            {
                out.push((node.start_byte(), node.end_byte()));
            }
            _ => {}
        }
        let mut c = node.walk();
        stack.extend(node.named_children(&mut c));
    }
    out.sort_unstable();
    out
}

/// Ranges of a Markdown file to check: everything outside code.
fn markdown_ranges(src: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut offset = 0;
    let mut fenced = false;
    for line in src.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced {
            // Inline `code` and links' targets are skipped.
            let mut in_code = false;
            let mut start = 0;
            for (i, c) in line.char_indices() {
                if c == '`' || c == '(' || c == ')' {
                    if !in_code && i > start {
                        out.push((offset + start, offset + i));
                    }
                    in_code = c == '`' && !in_code || c == '(';
                    start = i + 1;
                }
            }
            if !in_code && start < line.len() {
                out.push((offset + start, offset + line.len()));
            }
        }
        offset += line.len();
    }
    out
}

/// What kind of file is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// PHP: comments, strings and declared names.
    Php,
    /// Markdown: text outside code.
    Markdown,
    /// Plain text: everything.
    Text,
}

/// Typos in `src`, as weak problems (`Code::External("Spelling")`).
/// Runs on a background thread with the other inspections.
pub fn diagnostics(src: &str, kind: Kind) -> Vec<Diagnostic> {
    let ranges = match kind {
        Kind::Php => php_ranges(src),
        Kind::Markdown => markdown_ranges(src),
        Kind::Text => vec![(0, src.len())],
    };
    let mut found = Vec::new();
    for (a, b) in ranges {
        words(&src[a..b], a, &mut found);
    }
    let speller = speller().read();
    let lines = LineIndex::new(src);
    let mut seen_bad = HashSet::new();
    let mut out = Vec::new();
    for (start, end) in found {
        let word = &src[start..end];
        if seen_bad.contains(word) || !speller.accepts(word) {
            seen_bad.insert(word);
            out.push(Diagnostic {
                span: lines.span(src, start, end),
                start,
                end,
                severity: Severity::Weak,
                message: tr!("spelling-typo", word = word),
                code: Code::External("Spelling".into()),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typos(src: &str, kind: Kind) -> Vec<String> {
        diagnostics(src, kind)
            .into_iter()
            .map(|d| src[d.start..d.end].to_string())
            .collect()
    }

    #[test]
    fn words_should_split_camel_case_and_skip_acronyms() {
        let mut out = Vec::new();
        words("getUserNaem HTTPRequest id2 ok", 0, &mut out);
        let text = "getUserNaem HTTPRequest id2 ok";
        let found: Vec<&str> = out.iter().map(|(a, b)| &text[*a..*b]).collect();
        assert_eq!(found, ["get", "User", "Naem", "Request"]);
    }

    #[test]
    fn php_typos_should_come_from_comments_strings_and_declarations() {
        let src = "<?php\n// Recieve the payload\nfunction sendMesage($x) { return 'hello wrold'; }\n$y = strlen('teh_key');\n";
        assert_eq!(typos(src, Kind::Php), ["Recieve", "Mesage", "wrold"]);
    }

    #[test]
    fn doc_tags_should_only_check_descriptions() {
        let src = "<?php\n/**\n * @param int $usrId the identifer\n */\nfunction f($usrId) {}\n";
        assert_eq!(typos(src, Kind::Php), ["identifer"]);
    }

    #[test]
    fn portuguese_words_should_be_accepted() {
        let src = "<?php\n// Atualiza o contrato e envia a notificação ao locatário\n";
        assert!(typos(src, Kind::Php).is_empty());
    }

    #[test]
    fn markdown_code_should_be_skipped() {
        let src = "Teh title\n\n```php\n$recieve = 1;\n```\nUse `fooBarr` here.\n";
        assert_eq!(typos(src, Kind::Markdown), ["Teh"]);
    }
}
