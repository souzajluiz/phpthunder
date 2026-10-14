//! Code style rules from PSR-12 and PhpDoc consistency: `elseif`,
//! visibility on every member, modifier order, `new Foo()`, short casts,
//! one class per file, and `@param` tags naming real parameters.

use super::*;
use crate::syntax::{is_class_like, text};
use std::collections::HashSet;
use tree_sitter::Node;

/// Cast names PSR-12 writes short, with their short form.
pub(crate) const LONG_CASTS: &[(&str, &str)] = &[
    ("integer", "int"),
    ("boolean", "bool"),
    ("double", "float"),
    ("binary", "string"),
];

/// Runs every style rule over the tree rooted at `root`.
///
/// Called by [`diagnostics`] on each inspection pass.
pub(crate) fn psr_checks(root: Node, src: &str, out: &mut Findings) {
    let mut classes = 0;
    for node in descendants(root) {
        if node.is_error() {
            continue;
        }
        match node.kind() {
            "else_clause" => {
                if node
                    .child_by_field_name("body")
                    .is_some_and(|b| b.kind() == "if_statement")
                {
                    out.add(node.child(0).unwrap_or(node), Rule::ElseIf, "");
                }
            }
            "var_modifier" => out.add(node, Rule::VarKeyword, ""),
            "method_declaration" | "property_declaration" | "const_declaration" => {
                member_modifiers(node, src, out);
                if node.kind() == "method_declaration" {
                    doc_params(node, src, out);
                }
            }
            "object_creation_expression" => {
                let mut c = node.walk();
                let bare = !node
                    .named_children(&mut c)
                    .any(|n| matches!(n.kind(), "arguments" | "declaration_list"));
                if bare && !text(node, src).contains('(') {
                    out.add(node, Rule::NewWithoutParentheses, "");
                }
            }
            "cast_type" => {
                let name = text(node, src).trim().to_ascii_lowercase();
                if let Some((_, short)) = LONG_CASTS.iter().find(|(long, _)| *long == name) {
                    out.add(node, Rule::LongTypeCast, *short);
                }
            }
            kind if is_class_like(kind) => {
                classes += 1;
                if classes > 1
                    && let Some(name) = node.child_by_field_name("name")
                {
                    out.add(name, Rule::MultipleClassesInFile, "");
                }
            }
            "function_definition" => doc_params(node, src, out),
            _ => {}
        }
    }
}

/// PSR-12 rank of a modifier: `abstract`/`final`, then visibility, then
/// `static`/`readonly`.
fn modifier_rank(kind: &str) -> Option<u8> {
    match kind {
        "abstract_modifier" | "final_modifier" => Some(0),
        "visibility_modifier" => Some(1),
        "static_modifier" | "readonly_modifier" => Some(2),
        _ => None,
    }
}

/// Visibility present and modifiers in order, for one class member.
fn member_modifiers(member: Node, src: &str, out: &mut Findings) {
    let in_class = member
        .parent()
        .and_then(|p| p.parent())
        .is_some_and(|p| is_class_like(p.kind()));
    if !in_class {
        return;
    }
    let mut c = member.walk();
    let modifiers: Vec<Node> = member
        .children(&mut c)
        .filter(|n| modifier_rank(n.kind()).is_some())
        .collect();
    let has_var = {
        let mut c = member.walk();
        member.children(&mut c).any(|n| n.kind() == "var_modifier")
    };
    if !has_var && !modifiers.iter().any(|m| m.kind() == "visibility_modifier") {
        let name = match member.kind() {
            "method_declaration" => member.child_by_field_name("name"),
            _ => {
                let mut c = member.walk();
                member
                    .named_children(&mut c)
                    .find(|n| matches!(n.kind(), "property_element" | "const_element"))
                    .and_then(|e| e.named_child(0))
            }
        };
        if let Some(name) = name {
            out.add(name, Rule::MissingVisibility, text(name, src));
        }
    }
    let ranks: Vec<u8> = modifiers
        .iter()
        .filter_map(|m| modifier_rank(m.kind()))
        .collect();
    if ranks.windows(2).any(|w| w[0] > w[1]) {
        out.add_range(
            modifiers[0].start_byte(),
            modifiers[modifiers.len() - 1].end_byte(),
            Rule::ModifierOrder,
            "",
        );
    }
}

/// The doc comment right before `node`, if any.
pub(crate) fn doc_comment<'t>(node: Node<'t>, src: &str) -> Option<Node<'t>> {
    let prev = node.prev_named_sibling()?;
    (prev.kind() == "comment" && text(prev, src).starts_with("/**")).then_some(prev)
}

/// `@param` tags whose variable is not a parameter of the function.
pub(crate) fn doc_params(function: Node, src: &str, out: &mut Findings) {
    let (Some(doc), Some(params)) = (
        doc_comment(function, src),
        function.child_by_field_name("parameters"),
    ) else {
        return;
    };
    let mut c = params.walk();
    let names: HashSet<&str> = params
        .named_children(&mut c)
        .filter_map(|p| p.child_by_field_name("name"))
        .map(|n| text(n, src).trim_start_matches('&'))
        .collect();
    let doc_text = text(doc, src);
    let mut offset = doc.start_byte();
    for line in doc_text.split_inclusive('\n') {
        if let Some(at) = line.find("@param") {
            let rest = &line[at + 6..];
            if let Some((dollar, var)) = param_variable(rest) {
                let start = offset + at + 6 + dollar;
                if !names.contains(var) {
                    out.add_range(start, start + var.len(), Rule::PhpDocUnknownParameter, var);
                }
            }
        }
        offset += line.len();
    }
}

/// The `$name` a `@param` tag documents, with its offset in `rest` (the
/// text after `@param`): the first variable outside the type's brackets,
/// so `Closure(string $id): void $callback` yields `$callback`.
fn param_variable(rest: &str) -> Option<(usize, &str)> {
    let mut depth = 0i32;
    let bytes = rest.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'<' | b'{' | b'[' => depth += 1,
            b')' | b'>' | b'}' | b']' => depth -= 1,
            b'$' if depth <= 0 => {
                let len = rest[i + 1..]
                    .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .unwrap_or(rest.len() - i - 1);
                return (len > 0).then(|| (i, &rest[i..=i + len]));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn rules(src: &str) -> Vec<Rule> {
        let tree = parse(src);
        let mut out = Findings::default();
        psr_checks(tree.root_node(), src, &mut out);
        out.list.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn else_if_should_be_elseif() {
        assert_eq!(rules("<?php if ($a) {} else if ($b) {}"), [Rule::ElseIf]);
    }

    #[test]
    fn var_property_should_be_flagged() {
        assert_eq!(rules("<?php class A { var $x; }"), [Rule::VarKeyword]);
    }

    #[test]
    fn method_without_visibility_should_be_flagged() {
        assert_eq!(
            rules("<?php class A { function f() {} }"),
            [Rule::MissingVisibility]
        );
    }

    #[test]
    fn static_before_visibility_should_be_flagged() {
        assert_eq!(
            rules("<?php class A { static public function f() {} }"),
            [Rule::ModifierOrder]
        );
    }

    #[test]
    fn new_without_parentheses_should_be_flagged() {
        assert_eq!(rules("<?php $a = new Foo;"), [Rule::NewWithoutParentheses]);
    }

    #[test]
    fn long_cast_should_be_flagged() {
        assert_eq!(rules("<?php $a = (integer) $b;"), [Rule::LongTypeCast]);
    }

    #[test]
    fn second_class_in_file_should_be_flagged() {
        assert_eq!(
            rules("<?php class A {} class B {}"),
            [Rule::MultipleClassesInFile]
        );
    }

    #[test]
    fn param_tag_for_missing_parameter_should_be_flagged() {
        assert_eq!(
            rules(
                "<?php\n/**\n * @param int $a\n * @param string $gone\n */\nfunction f(int $a) {}"
            ),
            [Rule::PhpDocUnknownParameter]
        );
    }

    #[test]
    fn param_tag_with_callable_type_should_name_the_last_variable() {
        assert_eq!(
            rules(
                "<?php\n/**\n * @param Closure(string $id): array{a: int} $cb\n */\nfunction f(Closure $cb) {}"
            ),
            []
        );
    }
}
