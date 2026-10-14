//! Declaration-level context actions: `: void`, constructor property
//! promotion, `strict_types`, importing a qualified class name,
//! `#[\Override]` and `final`.

use super::*;
use crate::analysis::{Analyzer, Target};
use crate::index::member_key;
use crate::inspection::{descendants, remove_lines_edit};
use crate::symbol::{MemberTag, Visibility};
use crate::syntax::{context_at, import_insertion_point, is_class_like};

/// Collects the declaration actions that apply at the caret.
pub(super) fn collect(ctx: &Context, out: &mut Vec<Fix>) {
    out.extend(void_return_type(ctx));
    out.extend(promote_property(ctx));
    out.extend(strict_types(ctx));
    out.extend(import_class(ctx));
    out.extend(add_override(ctx));
    out.extend(make_final(ctx));
}

/// The function or method whose header (not body) holds the caret.
fn function_at_head<'t>(ctx: &Context<'t>) -> Option<Node<'t>> {
    ctx.ancestors().into_iter().find(|n| {
        matches!(n.kind(), "function_definition" | "method_declaration")
            && n.child_by_field_name("body")
                .is_none_or(|b| ctx.at < b.start_byte())
    })
}

/// The class whose header (not body) holds the caret.
fn class_at_head<'t>(ctx: &Context<'t>) -> Option<Node<'t>> {
    ctx.ancestors().into_iter().find(|n| {
        n.kind() == "class_declaration"
            && n.child_by_field_name("body")
                .is_some_and(|b| ctx.at < b.start_byte())
    })
}

/// Adds `: void` to a function without a return type that returns no value.
fn void_return_type(ctx: &Context) -> Option<Fix> {
    let function = function_at_head(ctx)?;
    if function.child_by_field_name("return_type").is_some() {
        return None;
    }
    let name = ctx
        .text(function.child_by_field_name("name")?)
        .to_ascii_lowercase();
    if matches!(name.as_str(), "__construct" | "__destruct" | "__clone") {
        return None;
    }
    let body = function.child_by_field_name("body")?;
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "anonymous_function"
            | "arrow_function"
            | "function_definition"
            | "class_declaration" => continue,
            "yield_expression" => return None,
            "return_statement" if n.named_child_count() > 0 => return None,
            _ => {}
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    let params = function.child_by_field_name("parameters")?;
    Some(rewrite(
        Action::AddVoidReturnType,
        params.end_byte(),
        params.end_byte(),
        ": void",
    ))
}

/// A constructor parameter copied into a property of the same name, to a
/// promoted parameter: the property declaration and the assignment go.
fn promote_property(ctx: &Context) -> Option<Fix> {
    let param = ctx.enclosing(&["simple_parameter"], false)?;
    let constructor = param.parent()?.parent()?;
    let is_constructor = constructor.kind() == "method_declaration"
        && constructor
            .child_by_field_name("name")
            .is_some_and(|n| ctx.text(n).eq_ignore_ascii_case("__construct"));
    if !is_constructor {
        return None;
    }
    let var = ctx.text(param.child_by_field_name("name")?);
    let prop = var.trim_start_matches('$');
    let body = constructor.child_by_field_name("body")?;
    let mut bc = body.walk();
    let assignment = body.named_children(&mut bc).find(|s| {
        s.kind() == "expression_statement"
            && s.named_child(0).is_some_and(|a| {
                a.kind() == "assignment_expression"
                    && a.child_by_field_name("left")
                        .is_some_and(|l| ctx.text(l) == format!("$this->{prop}"))
                    && a.child_by_field_name("right")
                        .is_some_and(|r| ctx.text(r) == var)
            })
    })?;
    let class_body = constructor.parent()?;
    let mut cc = class_body.walk();
    let declaration = class_body.named_children(&mut cc).find(|d| {
        if d.kind() != "property_declaration" {
            return false;
        }
        let mut dc = d.walk();
        let elements: Vec<Node> = d
            .named_children(&mut dc)
            .filter(|e| e.kind() == "property_element")
            .collect();
        matches!(elements.as_slice(), [e] if e.named_child_count() == 1
            && e.named_child(0).is_some_and(|v| ctx.text(v) == var))
    })?;
    let mut dc = declaration.walk();
    let modifiers: Vec<&str> = declaration
        .children(&mut dc)
        .filter(|n| matches!(n.kind(), "visibility_modifier" | "readonly_modifier"))
        .map(|n| ctx.text(n))
        .collect();
    if declaration
        .children(&mut declaration.walk())
        .any(|n| n.kind() == "static_modifier")
    {
        return None;
    }
    let modifiers = if modifiers
        .iter()
        .any(|m| matches!(*m, "public" | "protected" | "private"))
    {
        modifiers.join(" ")
    } else {
        std::iter::once("public")
            .chain(modifiers)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let param_text = ctx.text(param);
    let promoted = if param.child_by_field_name("type").is_none() {
        match declaration.child_by_field_name("type") {
            Some(t) => format!("{modifiers} {} {param_text}", ctx.text(t)),
            None => format!("{modifiers} {param_text}"),
        }
    } else {
        format!("{modifiers} {param_text}")
    };
    let mut edits = vec![
        Edit {
            path: None,
            start: param.start_byte(),
            end: param.end_byte(),
            text: promoted,
        },
        remove_lines_edit(ctx.src, assignment.start_byte(), assignment.end_byte()),
    ];
    let doc = declaration
        .prev_named_sibling()
        .filter(|d| d.kind() == "comment" && ctx.text(*d).starts_with("/**"));
    let start = doc.map_or(declaration.start_byte(), |d| d.start_byte());
    edits.push(remove_lines_edit(ctx.src, start, declaration.end_byte()));
    Some(rewrite_all(Action::PromoteProperty, edits))
}

/// Adds `declare(strict_types=1);` after the opening tag of a file that
/// lacks it, when the caret is at the top of the file.
fn strict_types(ctx: &Context) -> Option<Fix> {
    if ctx.src.contains("strict_types") {
        return None;
    }
    let mut c = ctx.root.walk();
    let children: Vec<Node> = ctx.root.named_children(&mut c).collect();
    let tag = children.first().filter(|t| t.kind() == "php_tag")?;
    let first = children.get(1).map_or(ctx.src.len(), |n| n.end_byte());
    (ctx.at <= first).then(|| {
        rewrite(
            Action::AddStrictTypes,
            tag.end_byte(),
            tag.end_byte(),
            "\n\ndeclare(strict_types=1);",
        )
    })
}

/// A qualified class name in code to `use` + the short name.
fn import_class(ctx: &Context) -> Option<Fix> {
    let qualified = ctx.enclosing(&["qualified_name"], true)?;
    let in_use = ctx.ancestors().iter().any(|n| {
        matches!(
            n.kind(),
            "namespace_use_declaration" | "namespace_definition"
        )
    });
    if in_use {
        return None;
    }
    let tree_ref = ctx.root;
    let last = descendants(qualified)
        .into_iter()
        .rfind(|n| n.kind() == "name")?;
    let analyzer = Analyzer::new(ctx.index, ctx.src, ctx.tree);
    let Some(Target::Class(fqn)) = analyzer.target_of(last) else {
        return None;
    };
    let short = fqn.rsplit('\\').next()?.to_string();
    let names = context_at(tree_ref, ctx.src, qualified.start_byte());
    let mut edits = vec![Edit {
        path: None,
        start: qualified.start_byte(),
        end: qualified.end_byte(),
        text: short.clone(),
    }];
    match names.classes.get(&short.to_ascii_lowercase()) {
        Some(existing) if existing.eq_ignore_ascii_case(&fqn) => {}
        Some(_) => return None,
        None => {
            // A class of that name in the current namespace would be shadowed.
            let at = import_insertion_point(tree_ref, qualified.start_byte());
            edits.push(Edit {
                path: None,
                start: at,
                end: at,
                text: format!("\nuse {fqn};"),
            });
        }
    }
    Some(rewrite_all(Action::ImportClass, edits))
}

/// Adds `#[\Override]` to a method that overrides an inherited one.
fn add_override(ctx: &Context) -> Option<Fix> {
    let method = function_at_head(ctx).filter(|m| m.kind() == "method_declaration")?;
    if method
        .child_by_field_name("attributes")
        .is_some_and(|a| ctx.text(a).contains("Override"))
    {
        return None;
    }
    let name = ctx.text(method.child_by_field_name("name")?);
    let mut class = method.parent()?;
    while !is_class_like(class.kind()) {
        class = class.parent()?;
    }
    // Supertypes as written in this file: the index may not have it yet.
    let names = context_at(ctx.root, ctx.src, class.start_byte());
    let mut c = class.walk();
    let supers: Vec<String> = class
        .named_children(&mut c)
        .filter(|n| matches!(n.kind(), "base_clause" | "class_interface_clause"))
        .flat_map(|clause| {
            let mut cc = clause.walk();
            clause
                .named_children(&mut cc)
                .filter(|n| matches!(n.kind(), "name" | "qualified_name"))
                .map(|n| names.resolve_class(ctx.text(n)))
                .collect::<Vec<_>>()
        })
        .collect();
    let overrides = supers
        .iter()
        .flat_map(|s| ctx.index.hierarchy(s))
        .any(|cls| {
            ctx.index
                .lookup(&member_key(&cls, MemberTag::Method, name))
                .iter()
                .any(|(_, s)| s.visibility != Visibility::Private && !s.is_virtual)
        });
    if !overrides {
        return None;
    }
    let indent = indent_at(ctx.src, method.start_byte());
    Some(rewrite(
        Action::AddOverride,
        method.start_byte(),
        method.start_byte(),
        format!("#[\\Override]\n{indent}"),
    ))
}

/// Adds `final` to a class that is neither final nor abstract.
fn make_final(ctx: &Context) -> Option<Fix> {
    let class = class_at_head(ctx)?;
    let mut c = class.walk();
    let children: Vec<Node> = class.children(&mut c).collect();
    if children
        .iter()
        .any(|n| matches!(n.kind(), "final_modifier" | "abstract_modifier"))
    {
        return None;
    }
    let first = children.iter().find(|n| n.kind() != "attribute_list")?;
    Some(rewrite(
        Action::MakeFinal,
        first.start_byte(),
        first.start_byte(),
        "final ",
    ))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{apply, apply_with};
    use super::*;
    use crate::index::index_source;
    use std::path::PathBuf;

    #[test]
    fn function_without_values_should_get_void() {
        assert_eq!(
            apply(
                "<?php function f|($a) { if ($a) { return; } echo 1; }",
                Action::AddVoidReturnType
            )
            .as_deref(),
            Some("<?php function f($a): void { if ($a) { return; } echo 1; }")
        );
    }

    #[test]
    fn function_returning_value_should_not_get_void() {
        assert_eq!(
            apply(
                "<?php function f|() { return 1; }",
                Action::AddVoidReturnType
            ),
            None
        );
    }

    #[test]
    fn assigned_parameter_should_be_promoted() {
        let src = "<?php\nclass A\n{\n    private string $name;\n\n    public function __construct(string $na|me)\n    {\n        $this->name = $name;\n    }\n}\n";
        assert_eq!(
            apply(src, Action::PromoteProperty).as_deref(),
            Some(
                "<?php\nclass A\n{\n\n    public function __construct(private string $name)\n    {\n    }\n}\n"
            )
        );
    }

    #[test]
    fn strict_types_should_follow_the_open_tag() {
        assert_eq!(
            apply("<?php|\n\nnamespace App;\n", Action::AddStrictTypes).as_deref(),
            Some("<?php\n\ndeclare(strict_types=1);\n\nnamespace App;\n")
        );
    }

    #[test]
    fn qualified_name_should_be_imported() {
        assert_eq!(
            apply(
                "<?php\nnamespace App;\n\n$u = new \\Lib\\Us|er();\n",
                Action::ImportClass
            )
            .as_deref(),
            Some("<?php\nnamespace App;\nuse Lib\\User;\n\n$u = new User();\n")
        );
    }

    #[test]
    fn overriding_method_should_get_override_attribute() {
        let mut index = Index::default();
        index.update(
            PathBuf::from("/b.php"),
            index_source("<?php class Base { public function run() {} }"),
        );
        let src = "<?php class C extends Base {\n    public function r|un() {}\n}";
        assert_eq!(
            apply_with(&index, src, Action::AddOverride).as_deref(),
            Some(
                "<?php class C extends Base {\n    #[\\Override]\n    public function run() {}\n}"
            )
        );
    }

    #[test]
    fn class_should_become_final() {
        assert_eq!(
            apply("<?php cl|ass A {}", Action::MakeFinal).as_deref(),
            Some("<?php final class A {}")
        );
    }
}
