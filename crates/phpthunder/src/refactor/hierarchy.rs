//! Type hierarchy queries: subtypes, overrides, and methods left to implement.

use crate::index::Index;
use crate::symbol::{Kind, MemberTag, Symbol};
use crate::syntax::{is_class_like, parse, text};
use std::path::PathBuf;

/// Members a class must still implement: abstract methods from its parent
/// chain and interface methods with no implementation in the hierarchy.
pub fn missing_implementations(index: &Index, class: &str) -> Vec<Symbol> {
    let mut missing = Vec::new();
    for cls in index.hierarchy(class).iter().skip(1) {
        let Some((path, decl)) = index.classes(cls).into_iter().next() else {
            continue;
        };
        for member in index
            .file_symbols(path)
            .iter()
            .filter(|s| s.kind == Kind::Method && s.fqn.eq_ignore_ascii_case(&decl.fqn))
        {
            // `@method` tags document magic methods; nothing must implement them.
            if member.is_virtual || !(decl.kind == Kind::Interface || member.is_abstract) {
                continue;
            }
            // Any concrete declaration in the hierarchy implements it: a
            // trait used by an ancestor counts even when an interface comes
            // earlier in the walk.
            let implemented = index.hierarchy(class).iter().any(|cls| {
                index
                    .lookup(&crate::index::member_key(
                        cls,
                        MemberTag::Method,
                        &member.name,
                    ))
                    .iter()
                    .any(|(_, s)| {
                        let owner_kind = index.classes(&s.fqn).first().map(|(_, c)| c.kind);
                        owner_kind != Some(Kind::Interface) && !s.is_abstract
                    })
            });
            if !implemented
                && !missing
                    .iter()
                    .any(|m: &Symbol| m.name.eq_ignore_ascii_case(&member.name))
            {
                missing.push(member.clone());
            }
        }
    }
    missing
}

/// Source for implementing `method` (signature without `abstract`, empty body).
pub fn implementation_stub(method: &Symbol) -> String {
    let header: Vec<&str> = method
        .signature
        .split_whitespace()
        .filter(|w| *w != "abstract")
        .collect();
    let mut header = header.join(" ");
    if !header.starts_with("public")
        && !header.starts_with("protected")
        && !header.starts_with("private")
    {
        header = format!("public {header}");
    }
    format!(
        "\n    {header}\n    {{\n        throw new \\RuntimeException('Not implemented');\n    }}\n"
    )
}

/// Classes that extend or implement `class` (directly or not): "go to
/// implementation" and the type hierarchy's subtype side.
pub fn subtypes(index: &Index, class: &str) -> Vec<(PathBuf, Symbol)> {
    let mut out = Vec::new();
    for (path, symbols) in index.files() {
        for s in symbols
            .iter()
            .filter(|s| s.kind.is_class_like() && !s.fqn.eq_ignore_ascii_case(class))
        {
            if index
                .hierarchy(&s.fqn)
                .iter()
                .skip(1)
                .any(|c| c.eq_ignore_ascii_case(class))
            {
                out.push((path.to_path_buf(), s.clone()));
            }
        }
    }
    out.sort_by(|a, b| a.1.fqn.cmp(&b.1.fqn));
    out
}

/// Methods that override or implement `method` of `class` in subtypes.
pub fn overriding_methods(index: &Index, class: &str, method: &str) -> Vec<(PathBuf, Symbol)> {
    subtypes(index, class)
        .into_iter()
        .flat_map(|(path, cls)| {
            index
                .file_symbols(&path)
                .iter()
                .filter(|s| {
                    s.kind == Kind::Method
                        && s.fqn.eq_ignore_ascii_case(&cls.fqn)
                        && s.name.eq_ignore_ascii_case(method)
                })
                .map(|s| (path.clone(), s.clone()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Whether `node` is inside a class-like declaration (used by callers that
/// generate members at the caret).
pub fn enclosing_class_fqn(src: &str, byte: usize) -> Option<String> {
    let tree = parse(src);
    let mut node = tree.root_node().descendant_for_byte_range(byte, byte)?;
    loop {
        if is_class_like(node.kind()) {
            let name = node.child_by_field_name("name")?;
            let ctx = crate::syntax::context_at(tree.root_node(), src, byte);
            return Some(ctx.qualify(text(name, src)));
        }
        node = node.parent()?;
    }
}
