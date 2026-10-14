//! PHP syntax highlighting from the tree-sitter tree.
//!
//! iced's editor asks for highlights one line at a time; we compute spans for
//! the whole document after each edit (tree-sitter parses typical files in
//! well under a millisecond) and hand them over through the highlighter
//! settings, versioned so the editor re-highlights only when they change.

use super::{Embedded, embedded_spans};
use crate::ui::{
    MONO_BOLD, SYN_COMMENT, SYN_DOC_TAG, SYN_FIELD, SYN_FUNCTION, SYN_KEYWORD, SYN_NUMBER,
    SYN_PUNCT, SYN_SEPARATOR, SYN_STRING, SYN_TYPE,
};
use iced::advanced::text::highlighter::{self, Format};
use iced::{Font, Theme};
use phpthunder::syntax::parse;
use std::ops::Range;
use std::sync::Arc;
use tree_sitter::Node;

/// Highlight class of a span; `format` maps it to a theme color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    /// Keywords, modifiers, `true`/`false`/`null`, primitive types and PHP tags.
    Keyword,
    /// Class, interface, trait, enum and namespace names in type position.
    Type,
    /// Function and method names at declarations and calls.
    Function,
    /// Variables, constants and enum cases.
    Variable,
    /// String literals, heredocs and nowdocs, escapes included.
    String,
    /// Integer and float literals.
    Number,
    /// Line, block and doc comments.
    Comment,
    /// `@tag` words inside `/** */` doc comments.
    DocTag,
    /// Member and scope operators: `->`, `?->`, `::`, `=>`.
    Punct,
    /// Commas and semicolons.
    Separator,
}

/// Per-line spans: the outer index is the line, ranges are byte offsets
/// within that line, sorted and non-overlapping.
pub type LineSpans = Vec<Vec<(Range<usize>, Token)>>;

/// Highlighter settings handed to the editor; built by
/// `Document::highlight_settings` from spans recomputed after every edit.
#[derive(Clone)]
pub struct HighlightSettings {
    /// Bumped by the document on every recompute.
    pub version: u64,
    /// The precomputed spans, shared without copying.
    pub lines: Arc<LineSpans>,
}

impl PartialEq for HighlightSettings {
    /// Each document and each edit gets a fresh allocation, so pointer
    /// identity tells whether the editor must re-highlight (also when the
    /// same editor widget switches to another tab).
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.lines, &other.lines) && self.version == other.version
    }
}

/// iced highlighter that replays precomputed `LineSpans` instead of
/// tokenizing each line itself.
pub struct PhpHighlighter {
    /// Spans for the whole document, from the latest settings.
    lines: Arc<LineSpans>,
    /// Next line the editor will ask for.
    current: usize,
}

impl highlighter::Highlighter for PhpHighlighter {
    type Settings = HighlightSettings;
    type Highlight = Token;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Token)>;

    fn new(settings: &HighlightSettings) -> Self {
        Self {
            lines: settings.lines.clone(),
            current: 0,
        }
    }

    fn update(&mut self, settings: &HighlightSettings) {
        self.lines = settings.lines.clone();
        self.current = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.current = self.current.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let spans = self.lines.get(self.current).cloned().unwrap_or_default();
        self.current += 1;
        // Spans may be stale for a moment while typing: never exceed the line.
        let len = line.len();
        spans
            .into_iter()
            .filter(|(r, _)| r.start < len)
            .map(|(r, t)| (r.start..r.end.min(len), t))
            .filter(|(r, _)| line.is_char_boundary(r.start) && line.is_char_boundary(r.end))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn current_line(&self) -> usize {
        self.current
    }
}

/// Maps a `Token` to its Thunder Purple syntax color, leaving the font
/// unchanged. Passed to `highlight_with` in the workbench `editor_area`.
pub fn format_token(token: &Token, _theme: &Theme) -> Format<Font> {
    let color = match token {
        Token::Keyword => SYN_KEYWORD,
        Token::Type => SYN_TYPE,
        Token::Function => SYN_FUNCTION,
        Token::Variable => SYN_FIELD,
        Token::String => SYN_STRING,
        Token::Number => SYN_NUMBER,
        Token::Comment => SYN_COMMENT,
        Token::DocTag => SYN_DOC_TAG,
        Token::Punct => SYN_PUNCT,
        Token::Separator => SYN_SEPARATOR,
    };
    let font = match token {
        Token::Separator => Some(MONO_BOLD),
        _ => None,
    };
    Format {
        color: Some(color),
        font,
    }
}

/// Picks the token for a tree-sitter leaf (or a whole string/comment node)
/// from its kind and its parent's kind; `None` leaves it uncolored. Anonymous
/// alphabetic nodes are keywords; `name` nodes are resolved by context
/// (declaration, call, type position, `Foo::` scope). `parent_node` is
/// passed in by the caller's walk: tree-sitter's `Node::parent` re-walks
/// from the root, which made highlighting large files slow.
fn classify(node: Node, parent_node: Option<Node>) -> Option<Token> {
    let kind = node.kind();
    let parent = parent_node.map(|p| p.kind()).unwrap_or("");
    if !node.is_named() {
        return match kind {
            "->" | "?->" | "::" | "=>" => Some(Token::Punct),
            "," | ";" => Some(Token::Separator),
            "$" if parent == "variable_name" => Some(Token::Variable),
            _ if kind.chars().all(|c| c.is_ascii_alphabetic() || c == '_') => Some(Token::Keyword),
            _ if kind.starts_with("<?") || kind == "?>" => Some(Token::Keyword),
            _ => None,
        };
    }
    match kind {
        "comment" => Some(Token::Comment),
        "string_content" | "string" | "encapsed_string" | "heredoc" | "nowdoc"
        | "escape_sequence" | "heredoc_body" | "nowdoc_body" | "string_value" => {
            Some(Token::String)
        }
        "integer" | "float" => Some(Token::Number),
        "boolean"
        | "null"
        | "primitive_type"
        | "php_tag"
        | "visibility_modifier"
        | "static_modifier"
        | "final_modifier"
        | "abstract_modifier"
        | "readonly_modifier"
        | "var_modifier" => Some(Token::Keyword),
        "name" => Some(match parent {
            "variable_name" => Token::Variable,
            "function_definition"
            | "method_declaration"
            | "function_call_expression"
            | "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression" => {
                // `Foo::bar()`: the scope is a type, the name is the function.
                let is_scope = parent == "scoped_call_expression"
                    && parent_node
                        .and_then(|p| p.child_by_field_name("scope"))
                        .is_some_and(|s| s.id() == node.id());
                if is_scope {
                    Token::Type
                } else {
                    Token::Function
                }
            }
            "class_declaration"
            | "interface_declaration"
            | "trait_declaration"
            | "enum_declaration"
            | "named_type"
            | "base_clause"
            | "class_interface_clause"
            | "object_creation_expression"
            | "qualified_name"
            | "namespace_name"
            | "namespace_use_clause"
            | "use_declaration"
            | "catch_clause"
            | "type_list"
            | "attribute"
            | "scoped_property_access_expression"
            | "binary_expression"
            | "optional_type"
            | "union_type" => Token::Type,
            "class_constant_access_expression" => {
                let is_scope = parent_node
                    .and_then(|p| p.named_child(0))
                    .is_some_and(|s| s.id() == node.id());
                if is_scope {
                    Token::Type
                } else {
                    Token::Variable
                }
            }
            "const_element" | "enum_case" => Token::Variable,
            _ => return None,
        }),
        _ => None,
    }
}

/// Records `start..end` (document byte offsets) as `token`, split across the
/// lines it covers. `starts` holds each line's start offset; newline bytes
/// are never included in a span.
pub(crate) fn push_span(
    lines: &mut LineSpans,
    starts: &[usize],
    src: &str,
    start: usize,
    end: usize,
    token: Token,
) {
    let mut pos = start;
    while pos < end {
        let line = starts.partition_point(|&s| s <= pos) - 1;
        let line_start = starts[line];
        let line_end = starts
            .get(line + 1)
            .map(|&s| s - 1)
            .unwrap_or(src.len())
            .max(line_start);
        let stop = end.min(line_end);
        if stop > pos {
            lines[line].push((pos - line_start..stop - line_start, token));
        }
        pos = line_end + 1;
    }
}

/// `@tag` words inside a doc comment.
fn doc_tags(src: &str, start: usize, end: usize, out: &mut Vec<(usize, usize)>) {
    let text = &src[start..end];
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@'
            && (i == 0 || bytes[i - 1].is_ascii_whitespace() || bytes[i - 1] == b'*')
        {
            let s = i;
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
            {
                i += 1;
            }
            if i > s + 1 {
                out.push((start + s, start + i));
            }
        } else {
            i += 1;
        }
    }
}

/// SQL statements a string literal may start with.
const SQL_STARTS: &[&str] = &[
    "select", "insert", "update", "delete", "with", "create", "alter", "drop", "replace",
    "truncate",
];

/// Words that, after an SQL start, confirm a query (so "Update your
/// profile" stays a plain string).
const SQL_CONFIRMS: &[&str] = &[
    " from ", " into ", " set ", " table ", " where ", " values", " join ", " as (",
];

/// Calls whose first argument is SQL whatever it looks like.
const SQL_CALLS: &[&str] = &[
    "raw",
    "selectraw",
    "whereraw",
    "orwhereraw",
    "havingraw",
    "orhavingraw",
    "orderbyraw",
    "groupbyraw",
    "fromraw",
    "statement",
    "unprepared",
    // Doctrine: DQL and DBAL.
    "createquery",
    "createnativequery",
    "executequery",
    "executestatement",
    "fetchallassociative",
    "fetchassociative",
    "fetchone",
];

/// The callee name of the call `string` is the first argument of, lowercased.
fn first_argument_of(string: Node, src: &str) -> Option<String> {
    let argument = string.parent().filter(|p| p.kind() == "argument")?;
    let arguments = argument.parent()?;
    if arguments.named_child(0)?.id() != argument.id() {
        return None;
    }
    let call = arguments.parent()?;
    let name = call
        .child_by_field_name("name")
        .or_else(|| call.child_by_field_name("function"))?;
    Some(
        src[name.byte_range()]
            .trim_start_matches('\\')
            .to_ascii_lowercase(),
    )
}

/// The language written inside a string literal, if any.
fn embedded_language(string: Node, content: &str, src: &str) -> Option<Embedded> {
    let callee = first_argument_of(string, src);
    if callee
        .as_deref()
        .is_some_and(|c| c.starts_with("preg_") && c != "preg_quote")
    {
        return Some(Embedded::Regex);
    }
    if callee.as_deref().is_some_and(|c| SQL_CALLS.contains(&c)) {
        return Some(Embedded::Sql);
    }
    let trimmed = content.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    let first = lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .next()
        .unwrap_or("");
    if SQL_STARTS.contains(&first) && SQL_CONFIRMS.iter().any(|w| lower.contains(w)) {
        return Some(Embedded::Sql);
    }
    let tag = trimmed.strip_prefix('<').and_then(|r| r.chars().next());
    if tag.is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!') && trimmed.contains('>')
    {
        return Some(Embedded::Html);
    }
    None
}

/// Spans of the language embedded in a string literal, as document
/// ranges; each piece of literal text is scanned on its own (around
/// interpolations).
fn injected(node: Node, src: &str) -> Vec<(usize, usize, Token)> {
    let pieces: Vec<(usize, usize)> = match node.kind() {
        "string" | "nowdoc" if node.end_byte() >= node.start_byte() + 2 => {
            let mut c = node.walk();
            let contents: Vec<(usize, usize)> = node
                .named_children(&mut c)
                .filter(|n| matches!(n.kind(), "string_content" | "nowdoc_body"))
                .map(|n| (n.start_byte(), n.end_byte()))
                .collect();
            if contents.is_empty() {
                vec![(node.start_byte() + 1, node.end_byte() - 1)]
            } else {
                contents
            }
        }
        _ => {
            let mut out = Vec::new();
            let mut stack = vec![node];
            while let Some(n) = stack.pop() {
                if n.kind() == "string_content" {
                    out.push((n.start_byte(), n.end_byte()));
                    continue;
                }
                let mut c = n.walk();
                stack.extend(n.named_children(&mut c));
            }
            out.sort_unstable();
            out
        }
    };
    let content: String = pieces.iter().map(|&(s, e)| &src[s..e]).collect();
    let Some(language) = embedded_language(node, &content, src) else {
        return Vec::new();
    };
    // A pattern is scanned whole: its delimiters are at both ends.
    if language == Embedded::Regex {
        let (Some(&(start, _)), Some(&(_, end))) = (pieces.first(), pieces.last()) else {
            return Vec::new();
        };
        return embedded_spans(&src[start..end], language)
            .into_iter()
            .map(|(s, e, t)| (start + s, start + e, t))
            .collect();
    }
    pieces
        .into_iter()
        .flat_map(|(start, end)| {
            embedded_spans(&src[start..end], language)
                .into_iter()
                .map(move |(s, e, t)| (start + s, start + e, t))
        })
        // The scanner's own string color would just repeat the literal's.
        .filter(|(_, _, t)| *t != Token::String)
        .collect()
}

/// Computes highlight spans for a whole PHP source. Called by
/// `Document::refresh_highlights` after every edit: parses with tree-sitter,
/// classifies leaves, adds doc-comment tags, then flattens overlaps per line
/// so nested spans (a tag inside a comment) split their parent.
pub fn compute_highlights(src: &str) -> LineSpans {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(src.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let mut lines: LineSpans = vec![Vec::new(); starts.len()];
    let tree = parse(src);
    let mut cursor = tree.walk();
    let mut stack: Vec<(Node, Option<Node>)> = vec![(tree.root_node(), None)];
    while let Some((node, parent)) = stack.pop() {
        // Strings and comments are colored as a whole; their children are not.
        let whole = matches!(
            node.kind(),
            "comment" | "string" | "encapsed_string" | "heredoc" | "nowdoc"
        );
        if node.child_count() > 0 && !whole {
            stack.extend(node.children(&mut cursor).map(|c| (c, Some(node))));
            continue;
        }
        let Some(token) = classify(node, parent) else {
            continue;
        };
        push_span(
            &mut lines,
            &starts,
            src,
            node.start_byte(),
            node.end_byte(),
            token,
        );
        if matches!(
            node.kind(),
            "string" | "encapsed_string" | "heredoc" | "nowdoc"
        ) {
            for (s, e, t) in injected(node, src) {
                push_span(&mut lines, &starts, src, s, e, t);
            }
        }
        if node.kind() == "comment" && src[node.byte_range()].starts_with("/**") {
            let mut tags = Vec::new();
            doc_tags(src, node.start_byte(), node.end_byte(), &mut tags);
            for (s, e) in tags {
                push_span(&mut lines, &starts, src, s, e, Token::DocTag);
            }
        }
    }
    for spans in &mut lines {
        spans.sort_by_key(|(r, _)| (r.start, std::cmp::Reverse(r.end)));
        // Later (more specific) spans like doc tags split the enclosing one.
        let mut flat: Vec<(Range<usize>, Token)> = Vec::new();
        for (r, t) in spans.drain(..) {
            if let Some(last) = flat.last_mut()
                && r.start < last.0.end
            {
                let tail = r.end..last.0.end;
                let outer = last.1;
                last.0.end = r.start;
                if last.0.is_empty() {
                    flat.pop();
                }
                flat.push((r, t));
                if !tail.is_empty() {
                    flat.push((tail, outer));
                }
                continue;
            }
            flat.push((r, t));
        }
        *spans = flat;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(src: &str) -> Vec<(String, Token)> {
        let lines = compute_highlights(src);
        let starts: Vec<usize> = std::iter::once(0)
            .chain(src.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        lines
            .iter()
            .enumerate()
            .flat_map(|(l, spans)| {
                let base = starts[l];
                spans
                    .iter()
                    .map(move |(r, t)| (src[base + r.start..base + r.end].to_string(), *t))
            })
            .collect()
    }

    #[test]
    fn sql_in_string_should_be_highlighted() {
        let t = tokens("<?php $q = \"SELECT id FROM users WHERE id = $id\";");
        assert!(t.iter().any(|(x, k)| x == "SELECT" && *k == Token::Keyword));
        assert!(t.iter().any(|(x, k)| x == "FROM" && *k == Token::Keyword));
    }

    #[test]
    fn plain_sentence_should_not_be_sql() {
        let t = tokens("<?php $q = 'Update your profile';");
        assert!(!t.iter().any(|(x, k)| x == "Update" && *k == Token::Keyword));
    }

    #[test]
    fn raw_query_argument_should_be_sql() {
        let t = tokens("<?php $q->whereRaw('id > ? and active = 1');");
        assert!(t.iter().any(|(x, k)| x == "and" && *k == Token::Keyword));
    }

    #[test]
    fn preg_pattern_should_be_highlighted() {
        let t = tokens("<?php preg_match('/^[a-z]+\\d{2}$/i', $s);");
        let has = |s: &str, tok: Token| t.iter().any(|(x, k)| x == s && *k == tok);
        assert!(has("[a-z]", Token::Type));
        assert!(has("\\d", Token::Number));
        assert!(has("{2}", Token::Punct));
        assert!(has("/i", Token::Keyword));
    }

    #[test]
    fn html_in_string_should_be_highlighted() {
        let t = tokens("<?php $h = '<div class=\"x\">hi</div>';");
        assert!(t.iter().any(|(x, _)| x == "div"));
    }

    #[test]
    fn highlights_php() {
        let src = "<?php\n/** @param Post $p */\nfunction show(Post $p): void { $p->save(); User::create('x'); }";
        let t = tokens(src);
        let has = |s: &str, tok: Token| t.iter().any(|(x, k)| x == s && *k == tok);
        assert!(has("<?php", Token::Keyword));
        assert!(has("@param", Token::DocTag));
        assert!(has("function", Token::Keyword));
        assert!(has("show", Token::Function));
        assert!(has("Post", Token::Type));
        assert!(has("void", Token::Keyword));
        assert!(has("save", Token::Function));
        assert!(has("User", Token::Type));
        assert!(has("create", Token::Function));
        assert!(has("'x'", Token::String));
        assert!(has("->", Token::Punct));
    }
}
