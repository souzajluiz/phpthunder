//! Parsing and name-resolution context (namespace + `use` imports).

use std::cell::RefCell;
use std::collections::HashMap;
use tree_sitter::{Node, Parser, Tree};

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new({
        let mut p = Parser::new();
        p.set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .expect("tree-sitter-php grammar is ABI compatible");
        p
    });
}

pub fn parse(src: &str) -> Tree {
    PARSER.with(|p| {
        p.borrow_mut()
            .parse(src, None)
            .expect("parser has a language")
    })
}

pub fn text<'a>(node: Node, src: &'a str) -> &'a str {
    &src[node.byte_range()]
}

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

/// Everything needed to turn a name written in source into a fully qualified name.
#[derive(Clone, Debug, Default)]
pub struct NameContext {
    pub namespace: String,
    /// lowercase alias -> fully qualified name
    pub classes: HashMap<String, String>,
    pub functions: HashMap<String, String>,
}

impl NameContext {
    pub fn qualify(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.to_string()
        } else {
            format!("{}\\{}", self.namespace, name)
        }
    }

    pub fn resolve_class(&self, raw: &str) -> String {
        if let Some(abs) = raw.strip_prefix('\\') {
            return abs.to_string();
        }
        if raw.len() > 10 && raw[..10].eq_ignore_ascii_case("namespace\\") {
            return self.qualify(&raw[10..]);
        }
        let (first, rest) = match raw.split_once('\\') {
            Some((f, r)) => (f, Some(r)),
            None => (raw, None),
        };
        match (self.classes.get(&first.to_ascii_lowercase()), rest) {
            (Some(target), Some(rest)) => format!("{target}\\{rest}"),
            (Some(target), None) => target.clone(),
            (None, _) => self.qualify(raw),
        }
    }

    /// Candidate FQNs in PHP lookup order: unqualified function calls fall back
    /// to the global namespace when the namespaced function does not exist.
    pub fn resolve_function(&self, raw: &str) -> Vec<String> {
        if raw.starts_with('\\') || raw.contains('\\') {
            return vec![self.resolve_class(raw)];
        }
        if let Some(target) = self.functions.get(&raw.to_ascii_lowercase()) {
            return vec![target.clone()];
        }
        let local = self.qualify(raw);
        if local == raw {
            vec![local]
        } else {
            vec![local, raw.to_string()]
        }
    }

    pub fn imports_class(&self, fqn: &str) -> bool {
        self.classes.values().any(|v| v.eq_ignore_ascii_case(fqn))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UseKind {
    Class,
    Function,
    Const,
}

fn use_kind(node: Node, src: &str) -> Option<UseKind> {
    node.child_by_field_name("type")
        .map(|t| match text(t, src) {
            "function" => UseKind::Function,
            "const" => UseKind::Const,
            _ => UseKind::Class,
        })
}

/// One clause of a `use` statement: `use A\B as C;` or one entry of `use A\{B, C};`.
pub struct UseClause<'t> {
    pub kind: UseKind,
    pub fqn: String,
    pub alias: String,
    pub name_node: Node<'t>,
    pub alias_node: Option<Node<'t>>,
}

pub fn use_clauses<'t>(decl: Node<'t>, src: &str) -> Vec<UseClause<'t>> {
    let decl_kind = use_kind(decl, src).unwrap_or(UseKind::Class);
    let mut prefix = String::new();
    let mut cursor = decl.walk();
    for child in decl.named_children(&mut cursor) {
        if child.kind() == "namespace_name" {
            prefix = text(child, src).trim_start_matches('\\').to_string();
        }
    }
    let mut out = Vec::new();
    let mut stack = vec![decl];
    while let Some(node) = stack.pop() {
        let mut c = node.walk();
        for child in node.named_children(&mut c) {
            match child.kind() {
                "namespace_use_group" => stack.push(child),
                "namespace_use_clause" => {
                    let alias_node = child.child_by_field_name("alias");
                    let mut cc = child.walk();
                    let Some(name_node) = child.named_children(&mut cc).find(|n| {
                        matches!(n.kind(), "name" | "qualified_name")
                            && Some(n.id()) != alias_node.map(|a| a.id())
                    }) else {
                        continue;
                    };
                    let name = text(name_node, src).trim_start_matches('\\');
                    let fqn = if prefix.is_empty() {
                        name.to_string()
                    } else {
                        format!("{prefix}\\{name}")
                    };
                    let alias = match alias_node {
                        Some(a) => text(a, src).to_string(),
                        None => fqn.rsplit('\\').next().unwrap_or(&fqn).to_string(),
                    };
                    out.push(UseClause {
                        kind: use_kind(child, src).unwrap_or(decl_kind),
                        fqn,
                        alias,
                        name_node,
                        alias_node,
                    });
                }
                _ => {}
            }
        }
    }
    out
}

fn add_uses(decl: Node, src: &str, ctx: &mut NameContext) {
    for clause in use_clauses(decl, src) {
        let key = clause.alias.to_ascii_lowercase();
        match clause.kind {
            UseKind::Class => {
                ctx.classes.insert(key, clause.fqn);
            }
            UseKind::Function => {
                ctx.functions.insert(key, clause.fqn);
            }
            UseKind::Const => {}
        }
    }
}

/// Visits every top-level statement (including those inside braced namespaces)
/// together with the name context in effect at that statement.
pub fn for_each_statement<'t>(
    root: Node<'t>,
    src: &str,
    mut f: impl FnMut(Node<'t>, &NameContext),
) {
    let mut ctx = NameContext::default();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "namespace_definition" {
            let name = child
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_default();
            let mut scoped = NameContext {
                namespace: name,
                ..Default::default()
            };
            if let Some(body) = child.child_by_field_name("body") {
                let mut c = body.walk();
                for stmt in body.named_children(&mut c) {
                    if stmt.kind() == "namespace_use_declaration" {
                        add_uses(stmt, src, &mut scoped);
                    }
                    f(stmt, &scoped);
                }
                ctx = NameContext::default();
            } else {
                ctx = scoped;
            }
            continue;
        }
        if child.kind() == "namespace_use_declaration" {
            add_uses(child, src, &mut ctx);
        }
        f(child, &ctx);
    }
}

/// Name context in effect at `byte`.
pub fn context_at(root: Node, src: &str, byte: usize) -> NameContext {
    let mut found = None;
    let mut last = NameContext::default();
    for_each_statement(root, src, |stmt, ctx| {
        if found.is_none() {
            if stmt.start_byte() <= byte && byte <= stmt.end_byte() {
                found = Some(ctx.clone());
            } else if stmt.start_byte() <= byte {
                last = ctx.clone();
            }
        }
    });
    found.unwrap_or(last)
}

/// Byte offset after which a new `use` statement for code at `byte` goes:
/// after the last `use`, else after the namespace header, else after `<?php`.
pub fn import_insertion_point(root: Node, byte: usize) -> usize {
    let mut point = None;
    let mut ns_end = None;
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.start_byte() > byte {
            break;
        }
        match child.kind() {
            "namespace_definition" => {
                point = None;
                ns_end = Some(child.end_byte());
                if let Some(body) = child.child_by_field_name("body")
                    && body.byte_range().contains(&byte)
                {
                    let mut c = body.walk();
                    let last_use = body
                        .named_children(&mut c)
                        .filter(|s| s.kind() == "namespace_use_declaration")
                        .last();
                    return last_use
                        .map(|u| u.end_byte())
                        .unwrap_or(body.start_byte() + 1);
                }
            }
            "namespace_use_declaration" => point = Some(child.end_byte()),
            "php_tag" if ns_end.is_none() && point.is_none() => point = Some(child.end_byte()),
            _ => {}
        }
    }
    point.or(ns_end).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_names_like_php() {
        let src = "<?php\nnamespace App;\nuse Lib\\Str as S;\nuse function Lib\\fmt;\nuse Lib\\{A, B as Bee};\nfoo();";
        let tree = parse(src);
        let ctx = context_at(tree.root_node(), src, src.len() - 2);
        assert_eq!(ctx.namespace, "App");
        assert_eq!(ctx.resolve_class("S"), "Lib\\Str");
        assert_eq!(ctx.resolve_class("S\\Inner"), "Lib\\Str\\Inner");
        assert_eq!(ctx.resolve_class("Bee"), "Lib\\B");
        assert_eq!(ctx.resolve_class("A"), "Lib\\A");
        assert_eq!(ctx.resolve_class("Other"), "App\\Other");
        assert_eq!(ctx.resolve_class("\\Root"), "Root");
        assert_eq!(ctx.resolve_function("fmt"), vec!["Lib\\fmt"]);
        assert_eq!(
            ctx.resolve_function("strlen"),
            vec!["App\\strlen", "strlen"]
        );
    }
}
