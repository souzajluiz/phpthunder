//! Parsing PHP source into a tree-sitter tree, and small node helpers.

use std::cell::RefCell;
use tree_sitter::{Node, Parser, Tree};

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new({
        let mut p = Parser::new();
        p.set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .expect("tree-sitter-php grammar is ABI compatible");
        p
    });
}

/// Parses PHP source with a thread-local tree-sitter parser, reused across calls
/// on the same thread (rayon workers each get their own).
///
/// Syntax errors become error nodes in the tree; it never fails on bad input.
///
/// # Panics
///
/// Only if the bundled grammar is ABI-incompatible with the tree-sitter runtime.
pub fn parse(src: &str) -> Tree {
    PARSER.with(|p| {
        p.borrow_mut()
            .parse(src, None)
            .expect("parser has a language")
    })
}

/// Source text of `node`.
///
/// # Panics
///
/// If `src` is not the text `node`'s tree was parsed from (out-of-range bytes).
pub fn text<'a>(node: Node, src: &'a str) -> &'a str {
    &src[node.byte_range()]
}

/// True for the tree-sitter declaration kinds of class, interface, trait, and enum.
pub fn is_class_like(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration" | "interface_declaration" | "trait_declaration" | "enum_declaration"
    )
}

/// Type names that never refer to a user-declared class.
pub fn is_builtin_type(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "int"
            | "float"
            | "string"
            | "bool"
            | "array"
            | "callable"
            | "iterable"
            | "object"
            | "mixed"
            | "void"
            | "null"
            | "never"
            | "false"
            | "true"
            | "integer"
            | "boolean"
            | "double"
    )
}
