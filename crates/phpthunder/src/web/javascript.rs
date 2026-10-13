//! JavaScript completion without a type checker: keywords, the browser
//! and framework globals (`document`, `Alpine`, `Livewire`, `axios`) with
//! their common members after `.`, and the names the file itself declares
//! or uses. Serves `.js`/`.ts`/`.vue` files, `<script>` blocks and Alpine
//! expressions.

use crate::analysis::Candidate;
use crate::symbol::Kind;
use std::collections::BTreeSet;

/// Keywords offered at the start of a word.
const KEYWORDS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "of",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "yield",
];

/// Globals, with what they are.
const GLOBALS: &[(&str, &str)] = &[
    ("window", "Window"),
    ("document", "Document"),
    ("console", "Console"),
    ("navigator", "Navigator"),
    ("location", "Location"),
    ("localStorage", "Storage"),
    ("sessionStorage", "Storage"),
    ("fetch", "function"),
    ("setTimeout", "function"),
    ("setInterval", "function"),
    ("clearTimeout", "function"),
    ("clearInterval", "function"),
    ("requestAnimationFrame", "function"),
    ("JSON", "JSON"),
    ("Math", "Math"),
    ("Object", "ObjectConstructor"),
    ("Array", "ArrayConstructor"),
    ("Promise", "PromiseConstructor"),
    ("Number", "NumberConstructor"),
    ("String", "StringConstructor"),
    ("Date", "DateConstructor"),
    ("URL", "URL"),
    ("URLSearchParams", "URLSearchParams"),
    ("FormData", "FormData"),
    ("Alpine", "Alpine.js"),
    ("Livewire", "Livewire"),
    ("axios", "axios"),
    ("Echo", "Laravel Echo"),
    ("route", "Ziggy route()"),
];

/// Members offered after `<global>.`.
const MEMBERS: &[(&str, &[&str])] = &[
    (
        "console",
        &[
            "log", "error", "warn", "info", "debug", "table", "dir", "group", "groupEnd", "time",
            "timeEnd", "trace", "assert",
        ],
    ),
    (
        "document",
        &[
            "querySelector",
            "querySelectorAll",
            "getElementById",
            "getElementsByClassName",
            "createElement",
            "addEventListener",
            "removeEventListener",
            "body",
            "head",
            "documentElement",
            "title",
            "cookie",
            "activeElement",
            "dispatchEvent",
        ],
    ),
    (
        "window",
        &[
            "location",
            "history",
            "addEventListener",
            "removeEventListener",
            "setTimeout",
            "setInterval",
            "fetch",
            "scrollTo",
            "scrollY",
            "innerWidth",
            "innerHeight",
            "matchMedia",
            "open",
            "dispatchEvent",
            "Alpine",
            "Livewire",
        ],
    ),
    ("JSON", &["parse", "stringify"]),
    (
        "Math",
        &[
            "abs", "ceil", "floor", "round", "max", "min", "random", "pow", "sqrt", "trunc",
            "sign", "PI",
        ],
    ),
    (
        "Object",
        &[
            "keys",
            "values",
            "entries",
            "assign",
            "freeze",
            "fromEntries",
            "create",
            "defineProperty",
        ],
    ),
    ("Array", &["isArray", "from", "of"]),
    (
        "Promise",
        &["all", "allSettled", "any", "race", "resolve", "reject"],
    ),
    (
        "Number",
        &["isInteger", "isNaN", "parseFloat", "parseInt", "isFinite"],
    ),
    (
        "localStorage",
        &["getItem", "setItem", "removeItem", "clear", "key", "length"],
    ),
    (
        "sessionStorage",
        &["getItem", "setItem", "removeItem", "clear", "key", "length"],
    ),
    (
        "location",
        &[
            "href", "pathname", "search", "hash", "origin", "reload", "assign", "replace",
        ],
    ),
    (
        "Alpine",
        &[
            "data",
            "store",
            "start",
            "plugin",
            "directive",
            "magic",
            "bind",
            "reactive",
            "effect",
            "nextTick",
            "evaluate",
            "morph",
        ],
    ),
    (
        "Livewire",
        &[
            "on",
            "dispatch",
            "dispatchTo",
            "hook",
            "find",
            "first",
            "getByName",
            "all",
            "start",
            "navigate",
            "directive",
        ],
    ),
    (
        "axios",
        &[
            "get",
            "post",
            "put",
            "patch",
            "delete",
            "request",
            "create",
            "defaults",
            "interceptors",
        ],
    ),
    (
        "Echo",
        &["channel", "private", "join", "leave", "listen", "socketId"],
    ),
];

/// Members of any array or string, offered after `.` on other receivers.
const COMMON_MEMBERS: &[&str] = &[
    "length",
    "map",
    "filter",
    "forEach",
    "find",
    "findIndex",
    "some",
    "every",
    "reduce",
    "includes",
    "indexOf",
    "push",
    "pop",
    "shift",
    "unshift",
    "slice",
    "splice",
    "join",
    "concat",
    "sort",
    "reverse",
    "flat",
    "flatMap",
    "at",
    "keys",
    "values",
    "entries",
    "split",
    "trim",
    "toLowerCase",
    "toUpperCase",
    "replace",
    "replaceAll",
    "startsWith",
    "endsWith",
    "padStart",
    "padEnd",
    "toFixed",
    "toString",
    "then",
    "catch",
    "finally",
    "addEventListener",
    "removeEventListener",
    "querySelector",
    "querySelectorAll",
    "classList",
    "dataset",
    "value",
    "textContent",
    "innerHTML",
    "focus",
    "blur",
    "click",
];

/// Whether `c` can be part of a JavaScript name.
fn is_name(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Start of the name ending at `offset`.
fn word_start(src: &str, offset: usize) -> usize {
    src[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_name(*c))
        .last()
        .map_or(offset, |(i, _)| i)
}

/// A completion item.
fn item(label: &str, kind: Kind, detail: &str) -> Candidate {
    Candidate {
        label: label.to_string(),
        kind,
        detail: detail.to_string(),
        insert_text: label.to_string(),
        import: None,
    }
}

/// Names used in `code`: identifiers after `const`/`let`/`var`/`function`/
/// `class`, object keys and every other word, for "words in file"
/// completion. Strings and comments are skipped roughly.
pub fn file_names(code: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    for c in code.chars() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        if matches!(c, '\'' | '"' | '`') {
            quote = Some(c);
        }
        if is_name(c) {
            word.push(c);
        } else if !word.is_empty() {
            if word.len() > 1 && !word.starts_with(|c: char| c.is_ascii_digit()) {
                out.insert(std::mem::take(&mut word));
            }
            word.clear();
        }
    }
    if word.len() > 1 {
        out.insert(word);
    }
    out
}

/// Completion at `offset` in JavaScript `code` (the whole file, or the
/// script part of a template): members after `.`, else keywords, globals
/// and the file's own names. `extra` adds context names (Alpine magics,
/// `x-data` keys). Returns where the replaced text starts.
pub fn javascript_completions(
    code: &str,
    offset: usize,
    extra: &[(String, String)],
) -> (usize, Vec<Candidate>) {
    let start = word_start(code, offset);
    let typed = &code[start..offset];
    let before = &code[..start];
    if let Some(receiver_end) = before.strip_suffix('.').map(str::len) {
        let receiver_start = word_start(code, receiver_end);
        let receiver = &code[receiver_start..receiver_end];
        let known = MEMBERS.iter().find(|(name, _)| *name == receiver);
        let items = match known {
            Some((_, members)) => members
                .iter()
                .map(|m| item(m, Kind::Method, receiver))
                .collect(),
            None => {
                // Members seen after a `.` in this file, plus the common ones.
                let mut names: BTreeSet<&str> = COMMON_MEMBERS.iter().copied().collect();
                let mut rest = code;
                while let Some(i) = rest.find('.') {
                    rest = &rest[i + 1..];
                    let end = rest.find(|c: char| !is_name(c)).unwrap_or(rest.len());
                    if end > 1 && rest.as_ptr() as usize - code.as_ptr() as usize != start {
                        names.insert(&rest[..end]);
                    }
                }
                names
                    .into_iter()
                    .filter(|n| *n != typed)
                    .map(|n| item(n, Kind::Property, "member"))
                    .collect()
            }
        };
        return (start, items);
    }
    let mut items: Vec<Candidate> = extra
        .iter()
        .map(|(name, detail)| item(name, Kind::Property, detail))
        .collect();
    items.extend(KEYWORDS.iter().map(|k| item(k, Kind::Constant, "keyword")));
    items.extend(GLOBALS.iter().map(|(g, d)| item(g, Kind::Class, d)));
    let mut seen: BTreeSet<String> = items.iter().map(|c| c.label.clone()).collect();
    for name in file_names(code) {
        if name != typed && seen.insert(name.clone()) {
            items.push(item(&name, Kind::Property, "in file"));
        }
    }
    (start, items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(src: &str) -> Vec<String> {
        let offset = src.find('|').unwrap();
        let src = src.replacen('|', "", 1);
        javascript_completions(&src, offset, &[])
            .1
            .into_iter()
            .map(|c| c.label)
            .collect()
    }

    #[test]
    fn javascript_completions_should_offer_global_members() {
        let got = labels("console.lo|");
        assert!(got.contains(&"log".to_string()) && !got.contains(&"const".to_string()));
    }

    #[test]
    fn javascript_completions_should_offer_file_names_and_keywords() {
        let got = labels("const counter = 1;\ncou|");
        assert!(got.contains(&"counter".to_string()) && got.contains(&"const".to_string()));
    }
}
