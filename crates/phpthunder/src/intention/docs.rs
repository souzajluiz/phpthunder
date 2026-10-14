//! Documentation and declaration generators offered at the caret: a
//! PHPDoc block from a signature, and PHP 8.4 property hooks.

use super::*;
use crate::inspection::descendants;

/// Collects the generators that apply at the caret.
pub(super) fn collect(ctx: &Context, out: &mut Vec<Fix>) {
    out.extend(phpdoc(ctx));
    out.extend(property_hooks(ctx));
}

/// The declaration (function, method or property) whose header holds the
/// caret.
fn declaration<'t>(ctx: &Context<'t>) -> Option<Node<'t>> {
    ctx.ancestors().into_iter().find(|n| match n.kind() {
        "function_definition" | "method_declaration" => n
            .child_by_field_name("body")
            .is_none_or(|b| ctx.at < b.start_byte()),
        "property_declaration" => true,
        _ => false,
    })
}

/// Whether a doc comment already precedes `node`.
fn documented(node: Node, src: &str) -> bool {
    node.prev_named_sibling()
        .is_some_and(|p| p.kind() == "comment" && text(p, src).starts_with("/**"))
}

/// A PHPDoc block listing parameters, return type and thrown exceptions
/// (or the property type), inserted above the declaration.
fn phpdoc(ctx: &Context) -> Option<Fix> {
    let decl = declaration(ctx)?;
    if documented(decl, ctx.src) {
        return None;
    }
    let mut tags: Vec<String> = Vec::new();
    if decl.kind() == "property_declaration" {
        let ty = decl
            .child_by_field_name("type")
            .map_or("mixed", |t| ctx.text(t));
        tags.push(format!("@var {ty}"));
    } else {
        let params = decl.child_by_field_name("parameters")?;
        let mut c = params.walk();
        for p in params
            .named_children(&mut c)
            .filter(|p| p.kind().ends_with("parameter"))
        {
            let ty = p
                .child_by_field_name("type")
                .map_or("mixed", |t| ctx.text(t));
            let name = p.child_by_field_name("name").map_or("", |n| ctx.text(n));
            let dots = if p.kind() == "variadic_parameter" {
                "..."
            } else {
                ""
            };
            tags.push(format!(
                "@param {ty} {dots}{}",
                name.trim_start_matches('&')
            ));
        }
        let body = decl.child_by_field_name("body");
        let returns_value = body.is_some_and(|b| {
            descendants(b)
                .iter()
                .any(|n| n.kind() == "return_statement" && n.named_child_count() > 0)
        });
        match decl.child_by_field_name("return_type") {
            Some(t) => tags.push(format!("@return {}", ctx.text(t))),
            None if returns_value => tags.push("@return mixed".into()),
            None => {}
        }
        let mut thrown: Vec<&str> = Vec::new();
        for n in body.map(descendants).unwrap_or_default() {
            if n.kind() == "throw_expression"
                && let Some(new) = n
                    .named_child(0)
                    .filter(|e| e.kind() == "object_creation_expression")
                && let Some(class) = new.named_child(0)
                && !thrown.contains(&ctx.text(class))
            {
                thrown.push(ctx.text(class));
            }
        }
        tags.extend(thrown.iter().map(|t| format!("@throws {t}")));
    }
    if tags.is_empty() {
        return None;
    }
    let indent = indent_at(ctx.src, decl.start_byte());
    let lines: String = tags.iter().map(|t| format!("{indent} * {t}\n")).collect();
    Some(rewrite(
        Action::GeneratePhpDoc,
        decl.start_byte(),
        decl.start_byte(),
        format!("/**\n{lines}{indent} */\n{indent}"),
    ))
}

/// `public string $name;` to a property with `get` and `set` hooks.
fn property_hooks(ctx: &Context) -> Option<Fix> {
    let decl = ctx.enclosing(&["property_declaration"], false)?;
    decl.child_by_field_name("type")?;
    let mut c = decl.walk();
    let children: Vec<Node> = decl.children(&mut c).collect();
    if children.iter().any(|n| {
        matches!(
            n.kind(),
            "static_modifier" | "readonly_modifier" | "property_hook_list"
        )
    }) {
        return None;
    }
    let elements: Vec<&Node> = children
        .iter()
        .filter(|n| n.kind() == "property_element")
        .collect();
    let [element] = elements[..] else {
        return None;
    };
    let name = ctx.text(element.named_child(0)?).trim_start_matches('$');
    let indent = indent_at(ctx.src, decl.start_byte());
    let unit = indent_unit(ctx.src);
    let head = ctx.src[decl.start_byte()..element.end_byte()].trim_end();
    Some(rewrite(
        Action::AddPropertyHooks,
        decl.start_byte(),
        decl.end_byte(),
        format!(
            "{head} {{\n{indent}{unit}get => $this->{name};\n{indent}{unit}set => $value;\n{indent}}}"
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::apply;
    use super::*;

    #[test]
    fn method_should_get_phpdoc_from_signature() {
        let src = "<?php\nclass A\n{\n    public function f|ind(int $id, ...$rest): ?User\n    {\n        throw new NotFound();\n    }\n}";
        assert_eq!(
            apply(src, Action::GeneratePhpDoc).as_deref(),
            Some(
                "<?php\nclass A\n{\n    /**\n     * @param int $id\n     * @param mixed ...$rest\n     * @return ?User\n     * @throws NotFound\n     */\n    public function find(int $id, ...$rest): ?User\n    {\n        throw new NotFound();\n    }\n}"
            )
        );
    }

    #[test]
    fn documented_function_should_not_get_another_phpdoc() {
        assert_eq!(
            apply(
                "<?php\n/** x */\nfunction f|($a) {}",
                Action::GeneratePhpDoc
            ),
            None
        );
    }

    #[test]
    fn typed_property_should_get_hooks() {
        let src = "<?php\nclass A\n{\n    public str|ing $name = '';\n}";
        assert_eq!(
            apply(src, Action::AddPropertyHooks).as_deref(),
            Some(
                "<?php\nclass A\n{\n    public string $name = '' {\n        get => $this->name;\n        set => $value;\n    }\n}"
            )
        );
    }
}
