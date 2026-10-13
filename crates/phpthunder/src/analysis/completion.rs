//! Completion: a placeholder identifier is inserted at the caret so incomplete
//! code parses, then the placeholder's position decides what to offer.

use super::*;
use crate::index::Index;
use crate::symbol::Kind;
use crate::syntax::{import_insertion_point, is_class_like, parse, text};
use std::collections::HashSet;
use tree_sitter::Node;

/// Identifier that [`complete`] splices in at the caret so incomplete code
/// (`$user->`) still parses with a `name` node where the caret is.
pub const CARET_PLACEHOLDER: &str = "PhpThunderCaret";

/// Completion proposals for the caret at `byte` in `src`.
///
/// After `$`: variables of the enclosing function. After `->`, `?->`, or `::`:
/// members of the inferred class. Elsewhere: top-level classes and functions
/// matching the typed prefix, with an auto-import edit when needed. Returns an
/// empty vec where nothing should be offered (declaration names, namespaced
/// names). Called by the server for every completion request; it parses `src`
/// with [`CARET_PLACEHOLDER`] inserted, so `src` is not modified.
pub fn complete(index: &Index, src: &str, byte: usize) -> Vec<Candidate> {
    // Variables need no placeholder: half-typed `$x` often confuses error recovery.
    let word = ident_start(src, byte);
    if word > 0 && src.as_bytes()[word - 1] == b'$' {
        return complete_variables(src, word - 1, byte);
    }
    let patched = format!("{}{}{}", &src[..byte], CARET_PLACEHOLDER, &src[byte..]);
    let tree = parse(&patched);
    let analyzer = Analyzer::new(index, &patched, &tree);
    let root = tree.root_node();
    let Some(mut node) = root.named_descendant_for_byte_range(byte, byte + 1) else {
        return Vec::new();
    };
    if node.kind() != "name" {
        return Vec::new();
    }
    let prefix = &patched[node.start_byte()..byte];
    let Some(mut parent) = node.parent() else {
        return Vec::new();
    };
    if matches!(parent.kind(), "namespace_name" | "qualified_name") {
        // Namespaced names are completed as a whole; keep it simple.
        return Vec::new();
    }
    if parent.kind() == "variable_name" {
        node = parent;
        parent = node.parent().unwrap_or(parent);
    }

    let members = |class: Option<String>, statics: bool| -> Vec<Candidate> {
        let Some(class) = class else {
            return Vec::new();
        };
        index
            .members(&class)
            .into_iter()
            .filter(|(_, s)| s.name.to_lowercase().starts_with(&prefix.to_lowercase()))
            .filter(|(_, s)| {
                if statics {
                    s.is_static
                } else {
                    s.kind == Kind::Method || !s.is_static
                }
            })
            .map(|(_, s)| {
                let name = if statics && s.kind == Kind::Property {
                    format!("${}", s.name)
                } else {
                    s.name.clone()
                };
                let insert = if s.kind == Kind::Method {
                    format!("{name}(")
                } else {
                    name.clone()
                };
                Candidate {
                    label: name,
                    kind: s.kind,
                    detail: s.signature.clone(),
                    insert_text: insert,
                    import: None,
                }
            })
            .collect()
    };

    // `Foo::|`: the scope is the first child, the caret is on the second.
    let constant_scope = (parent.kind() == "class_constant_access_expression")
        .then(|| parent.named_child(0))
        .flatten()
        .filter(|s| s.id() != node.id());
    match parent.kind() {
        "member_call_expression"
        | "member_access_expression"
        | "nullsafe_member_call_expression"
        | "nullsafe_member_access_expression" => {
            let object = parent.child_by_field_name("object");
            members(object.and_then(|o| analyzer.type_of(o, 0)), false)
        }
        "scoped_call_expression" | "scoped_property_access_expression" => {
            let scope = parent.child_by_field_name("scope");
            members(scope.and_then(|s| analyzer.class_of_scope(s, 0)), true)
        }
        "class_constant_access_expression" if constant_scope.is_some() => {
            let Some(scope) = constant_scope else {
                return Vec::new();
            };
            let mut out = members(analyzer.class_of_scope(scope, 0), true);
            if "class".starts_with(&prefix.to_lowercase()) {
                out.push(Candidate {
                    label: "class".into(),
                    kind: Kind::ClassConstant,
                    detail: "class name".into(),
                    insert_text: "class".into(),
                    import: None,
                });
            }
            out
        }
        k if is_class_like(k)
            || matches!(
                k,
                "function_definition"
                    | "method_declaration"
                    | "const_element"
                    | "namespace_definition"
            ) =>
        {
            Vec::new()
        }
        _ => complete_names(&analyzer, node, prefix, src, byte, parent.kind()),
    }
}

/// Start of the identifier that ends at `byte` (without a leading `$`).
///
/// Used by [`complete`] to detect variables and by the server to compute the
/// range of the typed prefix that a completion replaces.
pub fn ident_start(src: &str, byte: usize) -> usize {
    let bytes = src.as_bytes();
    let mut i = byte;
    while i > 0
        && (bytes[i - 1] == b'_' || bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] >= 0x80)
    {
        i -= 1;
    }
    i
}

/// Variables visible in the function around `start` (the `$` being typed).
fn complete_variables(src: &str, start: usize, byte: usize) -> Vec<Candidate> {
    let prefix = &src[start..byte];
    let tree = parse(src);
    let here = tree.root_node().descendant_for_byte_range(start, start);
    let function = here.and_then(enclosing_function);
    let scope = function.unwrap_or(tree.root_node());
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            if n.id() == scope.id() || !is_function_like(child.kind()) {
                stack.push(child);
            }
        }
        if n.kind() == "variable_name" && n.start_byte() != start {
            names.push(text(n, src));
        }
    }
    if function.is_some_and(|f| f.kind() == "method_declaration") {
        names.push("$this");
    }
    names
        .into_iter()
        .filter(|name| name.starts_with(prefix) && *name != prefix && seen.insert(*name))
        .map(|name| Candidate {
            label: name.to_string(),
            kind: Kind::Property,
            detail: "variable".into(),
            insert_text: name.to_string(),
            import: None,
        })
        .collect()
}

/// Top-level classes and functions whose short name starts with `prefix`, for
/// a bare name at `node` inside the placeholder-patched tree.
///
/// In type positions (`new`, `extends`, type hints, ...) only class-likes are
/// offered. A class from another namespace gets a `use` import edit, placed
/// relative to `original` (the unpatched source) at `byte`; if the alias
/// clashes, or for functions in another namespace, the FQN is inserted instead.
/// Capped at 200 index hits.
fn complete_names(
    analyzer: &Analyzer,
    node: Node,
    prefix: &str,
    original: &str,
    byte: usize,
    parent_kind: &str,
) -> Vec<Candidate> {
    let ctx = analyzer.ctx(node.start_byte());
    // Function calls are possible only where an expression is.
    let type_position = matches!(
        parent_kind,
        "named_type"
            | "base_clause"
            | "class_interface_clause"
            | "object_creation_expression"
            | "catch_clause"
            | "type_list"
            | "attribute"
            | "use_declaration"
            | "optional_type"
            | "union_type"
    );
    let original_tree = parse(original);
    let import_at = import_insertion_point(original_tree.root_node(), byte);
    analyzer
        .index
        .top_level_with_prefix(prefix, 200)
        .into_iter()
        .filter(|(_, s)| s.kind.is_class_like() || !type_position)
        .map(|(_, s)| {
            let short = s.fqn.rsplit('\\').next().unwrap_or(&s.fqn).to_string();
            let namespace = s.fqn.rsplit_once('\\').map(|(ns, _)| ns).unwrap_or("");
            let foreign = !namespace.eq_ignore_ascii_case(&ctx.namespace);
            let mut import = None;
            let mut insert = short.clone();
            if s.kind.is_class_like() {
                if foreign && !ctx.imports_class(&s.fqn) {
                    if ctx.classes.contains_key(&short.to_ascii_lowercase()) {
                        insert = format!("\\{}", s.fqn); // alias clash: fully qualify
                    } else if !namespace.is_empty() || !ctx.namespace.is_empty() {
                        import = Some((import_at, format!("\nuse {};", s.fqn)));
                    }
                }
            } else if foreign && !namespace.is_empty() {
                insert = format!("\\{}", s.fqn);
            }
            if s.kind == Kind::Function {
                insert.push('(');
            }
            Candidate {
                label: short,
                kind: s.kind,
                detail: s.fqn.clone(),
                insert_text: insert,
                import,
            }
        })
        .collect()
}
