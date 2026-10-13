//! PHP syntax highlighting from the tree-sitter tree.
//!
//! iced's editor asks for highlights one line at a time; we compute spans for
//! the whole document after each edit (tree-sitter parses typical files in
//! well under a millisecond) and hand them over through the highlighter
//! settings, versioned so the editor re-highlights only when they change.

use crate::ui::{
    SYN_COMMENT, SYN_DOC_TAG, SYN_FUNCTION, SYN_KEYWORD, SYN_NUMBER, SYN_PUNCT, SYN_STRING,
    SYN_TYPE, SYN_VARIABLE,
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

/// Maps a `Token` to its Thunder Dark syntax color, leaving the font
/// unchanged. Passed to `highlight_with` in the workbench `editor_area`.
pub fn format_token(token: &Token, _theme: &Theme) -> Format<Font> {
    let color = match token {
        Token::Keyword => SYN_KEYWORD,
        Token::Type => SYN_TYPE,
        Token::Function => SYN_FUNCTION,
        Token::Variable => SYN_VARIABLE,
        Token::String => SYN_STRING,
        Token::Number => SYN_NUMBER,
        Token::Comment => SYN_COMMENT,
        Token::DocTag => SYN_DOC_TAG,
        Token::Punct => SYN_PUNCT,
    };
    Format {
        color: Some(color),
        font: None,
    }
}

/// Picks the token for a tree-sitter leaf (or a whole string/comment node)
/// from its kind and its parent's kind; `None` leaves it uncolored. Anonymous
/// alphabetic nodes are keywords; `name` nodes are resolved by context
/// (declaration, call, type position, `Foo::` scope).
fn classify(node: Node) -> Option<Token> {
    let kind = node.kind();
    let parent = node.parent().map(|p| p.kind()).unwrap_or("");
    if !node.is_named() {
        return match kind {
            "->" | "?->" | "::" | "=>" => Some(Token::Punct),
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
            | "scoped_call_expression"
                if node.next_sibling().is_none_or(|s| s.kind() != "::") =>
            {
                // `Foo::bar()`: the scope is a type, the name is the function.
                let is_scope = parent == "scoped_call_expression"
                    && node
                        .parent()
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
                let is_scope = node
                    .parent()
                    .and_then(|p| p.named_child(0))
                    .is_some_and(|s| s.id() == node.id());
                if is_scope {
                    Token::Type
                } else {
                    Token::Variable
                }
            }
            "scoped_call_expression" => Token::Type,
            "const_element" | "enum_case" => Token::Variable,
            _ => return None,
        }),
        _ => None,
    }
}

/// Records `start..end` (document byte offsets) as `token`, split across the
/// lines it covers. `starts` holds each line's start offset; newline bytes
/// are never included in a span.
fn push_span(
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
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        // Strings and comments are colored as a whole; their children are not.
        let whole = matches!(
            node.kind(),
            "comment" | "string" | "encapsed_string" | "heredoc" | "nowdoc"
        );
        if node.child_count() > 0 && !whole {
            stack.extend(node.children(&mut cursor));
            continue;
        }
        let Some(token) = classify(node) else {
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
