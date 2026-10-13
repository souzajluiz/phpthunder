//! Completion: a placeholder identifier is inserted at the caret so incomplete
//! code parses, then the placeholder's position decides what to offer.

use super::*;
use crate::index::Index;
use crate::laravel;
use crate::symbol::{Kind, Symbol, Ty, Visibility};
use crate::syntax::{import_insertion_point, is_class_like, parse, text};
use std::collections::HashSet;
use tree_sitter::Node;

/// Identifier that [`complete`] splices in at the caret so incomplete code
/// (`$user->`) still parses with a `name` node where the caret is.
pub const CARET_PLACEHOLDER: &str = "PhpThunderCaret";

/// Completion proposals for the caret at `byte` in `src`.
///
/// After `$`: variables of the enclosing function. After `->`, `?->`, or `::`:
/// members of the inferred class. Elsewhere: top-level classes and functions,
/// with an auto-import edit when needed. Names match the typed text as a
/// prefix, by camel humps or by a word in the middle ([`Matcher`]). Returns an
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
    let matcher = Matcher::new(prefix);
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

    // Classes whose protected members the caret can reach: the enclosing
    // class and its supertypes.
    let scope_class = analyzer.enclosing_class(node, &analyzer.ctx(byte));
    let reach: HashSet<String> = scope_class
        .iter()
        .flat_map(|c| index.hierarchy(c))
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let visible = |s: &Symbol| match s.visibility {
        Visibility::Public => true,
        Visibility::Protected => reach.contains(&s.fqn.to_ascii_lowercase()),
        Visibility::Private => scope_class
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case(&s.fqn)),
    };
    let members = |receiver: Ty, statics: bool| -> Vec<Candidate> {
        let Some(class) = receiver.class.clone() else {
            return Vec::new();
        };
        let mut found: Vec<&Symbol> = index
            .members(&class)
            .into_iter()
            .map(|(_, s)| s)
            .filter(|s| visible(s))
            .filter(|s| {
                // Static members are reachable through `->` in PHP, but are
                // noise there; `::` lists only them.
                statics == s.is_static
            })
            .collect();
        // Eloquent forwarding: `User::where()` reaches the query builder,
        // and a `Builder<User>` reaches the model's scopes.
        if let Some(next) = laravel::forward(index, &receiver, crate::symbol::MemberTag::Method)
            .and_then(|t| t.class)
        {
            let model_to_builder = laravel::is_model(index, &class);
            if statics || !model_to_builder {
                found.extend(
                    index
                        .members(&next)
                        .into_iter()
                        .map(|(_, s)| s)
                        .filter(|s| s.kind == Kind::Method && visible(s))
                        .filter(|s| !s.name.starts_with("__"))
                        .filter(|s| model_to_builder || (s.is_virtual && s.is_static)),
                );
            }
        }
        let mut seen = HashSet::new();
        found
            .into_iter()
            .filter(|s| matcher.matches(&s.name).is_some())
            .filter(|s| seen.insert((s.kind == Kind::Method, s.name.to_ascii_lowercase())))
            .map(|s| {
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
            members(
                object.map(|o| analyzer.ty_of(o, 0)).unwrap_or_default(),
                false,
            )
        }
        "scoped_call_expression" | "scoped_property_access_expression" => {
            let scope = parent.child_by_field_name("scope");
            let class = scope.and_then(|s| analyzer.class_of_scope(s, 0));
            members(
                Ty {
                    class,
                    ..Ty::default()
                },
                true,
            )
        }
        "class_constant_access_expression" if constant_scope.is_some() => {
            let Some(scope) = constant_scope else {
                return Vec::new();
            };
            let class = analyzer.class_of_scope(scope, 0);
            let mut out = members(
                Ty {
                    class,
                    ..Ty::default()
                },
                true,
            );
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

/// Most class and function names offered for a bare identifier.
const MAX_NAME_CANDIDATES: usize = 200;

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
    let matcher = Matcher::new(prefix);
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
        .filter(|name| *name != prefix && matcher.matches(name).is_some() && seen.insert(*name))
        .map(|name| Candidate {
            label: name.to_string(),
            kind: Kind::Property,
            detail: "variable".into(),
            insert_text: name.to_string(),
            import: None,
        })
        .collect()
}

/// Top-level classes and functions whose short name matches `prefix` (see
/// [`Matcher`]), for a bare name at `node` inside the placeholder-patched tree.
///
/// In type positions (`new`, `extends`, type hints, ...) only class-likes are
/// offered. A class from another namespace gets a `use` import edit, placed
/// relative to `original` (the unpatched source) at `byte`; if the alias
/// clashes, or for functions in another namespace, the FQN is inserted instead.
/// The best [`MAX_NAME_CANDIDATES`] matches are kept.
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
    // Prefix, camel-hump and middle-word matches, best first, capped at
    // `MAX_NAME_CANDIDATES` so a short prefix in a large project stays fast.
    let matcher = Matcher::new(prefix);
    let mut hits: Vec<(MatchTier, &Symbol)> = analyzer
        .index
        .top_level()
        .filter(|(_, s)| s.kind.is_class_like() || !type_position)
        .filter_map(|(_, s)| {
            let short = s.fqn.rsplit('\\').next().unwrap_or(&s.fqn);
            Some((matcher.matches(short)?.tier, s))
        })
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.truncate(MAX_NAME_CANDIDATES);
    hits.into_iter()
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
