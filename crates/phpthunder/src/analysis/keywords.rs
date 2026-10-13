//! PHP keyword proposals for completion: offered where a statement or an
//! expression can start, not after `->`, `::`, `$` or `\`, nor where a
//! declaration is being named.

use super::*;
use crate::symbol::Kind;

/// Keywords and what completing them inserts. A trailing space leaves room
/// for what follows; `(` asks the editor for parentheses with the caret
/// inside, as for a call.
const KEYWORDS: &[(&str, &str)] = &[
    ("abstract", "abstract "),
    ("array", "array("),
    ("as", "as "),
    ("break", "break;"),
    ("case", "case "),
    ("catch", "catch ("),
    ("class", "class "),
    ("clone", "clone "),
    ("const", "const "),
    ("continue", "continue;"),
    ("default", "default"),
    ("do", "do "),
    ("echo", "echo "),
    ("else", "else "),
    ("elseif", "elseif ("),
    ("empty", "empty("),
    ("enum", "enum "),
    ("extends", "extends "),
    ("false", "false"),
    ("final", "final "),
    ("finally", "finally "),
    ("fn", "fn("),
    ("for", "for ("),
    ("foreach", "foreach ("),
    ("function", "function "),
    ("global", "global "),
    ("if", "if ("),
    ("implements", "implements "),
    ("include_once", "include_once "),
    ("instanceof", "instanceof "),
    ("interface", "interface "),
    ("isset", "isset("),
    ("list", "list("),
    ("match", "match ("),
    ("namespace", "namespace "),
    ("new", "new "),
    ("null", "null"),
    ("parent", "parent"),
    ("private", "private "),
    ("protected", "protected "),
    ("public", "public "),
    ("readonly", "readonly "),
    ("require_once", "require_once "),
    ("return", "return "),
    ("self", "self"),
    ("static", "static "),
    ("switch", "switch ("),
    ("throw", "throw "),
    ("trait", "trait "),
    ("true", "true"),
    ("try", "try "),
    ("unset", "unset("),
    ("use", "use "),
    ("while", "while ("),
    ("yield", "yield "),
];

/// Words after which the next word names a declaration, so no keyword fits.
const DECLARING: &[&str] = &[
    "function",
    "class",
    "interface",
    "trait",
    "enum",
    "const",
    "namespace",
    "fn",
];

/// Keyword proposals for the caret at `byte` in `src` (unfiltered: the
/// caller matches them against the typed prefix). Empty where keywords
/// cannot appear.
pub fn keyword_candidates(src: &str, byte: usize) -> Vec<Candidate> {
    let start = ident_start(src, byte);
    let before = &src[..start];
    if before.ends_with(['$', '\\']) || before.ends_with("->") || before.ends_with("::") {
        return Vec::new();
    }
    let previous_word = before
        .trim_end()
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or_default();
    if before.ends_with(char::is_whitespace)
        && DECLARING.contains(&previous_word.to_ascii_lowercase().as_str())
    {
        return Vec::new();
    }
    KEYWORDS
        .iter()
        .map(|(keyword, insert)| Candidate {
            label: (*keyword).to_string(),
            kind: Kind::Constant,
            detail: KEYWORD_DETAIL.to_string(),
            insert_text: (*insert).to_string(),
            import: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(src: &str) -> bool {
        !keyword_candidates(src, src.len()).is_empty()
    }

    #[test]
    fn keywords_should_appear_only_where_statements_or_expressions_start() {
        assert!(offered("<?php\nfunction a() {\n    ret"));
        assert!(offered("<?php\n$x = ne"));
        assert!(!offered("<?php\n$user->ret"));
        assert!(!offered("<?php\nUser::ret"));
        assert!(!offered("<?php\n$ret"));
        assert!(!offered("<?php\nfunction ret"));
        assert!(!offered("<?php\nuse App\\Ret"));
    }
}
