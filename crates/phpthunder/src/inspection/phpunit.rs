//! PHPUnit assertions that a dedicated assertion says better:
//! `assertTrue(empty($x))` is `assertEmpty($x)`, `assertEquals(3,
//! count($x))` is `assertCount(3, $x)`, and so on, plus expected and
//! actual values passed in the wrong order. Matched by method name on
//! `$this->`, `self::` and `static::` calls.

use super::*;
use crate::syntax::text;
use tree_sitter::Node;

/// A better assertion for an assertion call: the new method name and
/// argument sources (the message argument, if any, is kept after them).
pub(crate) struct Rewrite {
    /// Which rule suggests it.
    pub rule: Rule,
    /// Replacement method name.
    pub method: String,
    /// Replacement arguments, in order, as source text.
    pub args: Vec<String>,
}

/// Runs the PHPUnit rules over the tree rooted at `root`.
///
/// Called by [`diagnostics`] on each inspection pass.
pub(crate) fn phpunit_checks(root: Node, src: &str, out: &mut Findings) {
    for node in descendants(root) {
        if matches!(
            node.kind(),
            "member_call_expression" | "scoped_call_expression"
        ) && let Some(rewrite) = assertion_rewrite(node, src)
        {
            out.add(node, rewrite.rule, rewrite.method);
        }
    }
}

/// Whether `node` is a literal value (expected values usually are).
fn is_literal(node: Node, src: &str) -> bool {
    let node = unparen(node);
    match node.kind() {
        "string" | "integer" | "float" | "boolean" | "null" => true,
        "encapsed_string" => literal_string(node, src).is_some(),
        "unary_op_expression" => node
            .child_by_field_name("argument")
            .is_some_and(|a| matches!(a.kind(), "integer" | "float")),
        _ => false,
    }
}

/// The better assertion for an assertion call, if one applies.
pub(crate) fn assertion_rewrite(call: Node, src: &str) -> Option<Rewrite> {
    let receiver = call
        .child_by_field_name("object")
        .or_else(|| call.child_by_field_name("scope"))?;
    if !matches!(text(receiver, src), "$this" | "self" | "static") {
        return None;
    }
    let method = text(call.child_by_field_name("name")?, src);
    let args = call_args(call)?;
    let src_of = |n: Node| text(n, src).to_string();
    let rewrite = |rule, method: &str, args: Vec<String>| {
        Some(Rewrite {
            rule,
            method: method.to_string(),
            args,
        })
    };
    let lower = method.to_ascii_lowercase();
    let is_true = lower == "asserttrue";
    match lower.as_str() {
        "assertequals" | "assertsame" | "assertnotequals" | "assertnotsame" if args.len() >= 2 => {
            let negated = lower.contains("not");
            let expected = text(unparen(args[0]), src).to_ascii_lowercase();
            let literal_method = match (expected.as_str(), negated) {
                ("true", false) => Some("assertTrue"),
                ("false", false) => Some("assertFalse"),
                ("null", false) => Some("assertNull"),
                ("null", true) => Some("assertNotNull"),
                _ => None,
            };
            if let Some(m) = literal_method {
                return rewrite(Rule::AssertEqualsLiteral, m, vec![src_of(args[1])]);
            }
            let actual = unparen(args[1]);
            if !negated
                && call_name(actual, src).as_deref() == Some("count")
                && let Some(inner) = call_args(actual).filter(|a| a.len() == 1)
            {
                return rewrite(
                    Rule::AssertCount,
                    "assertCount",
                    vec![src_of(args[0]), src_of(inner[0])],
                );
            }
            if is_literal(args[1], src) && !is_literal(args[0], src) {
                return rewrite(
                    Rule::AssertArgumentsOrder,
                    method,
                    vec![src_of(args[1]), src_of(args[0])],
                );
            }
            None
        }
        "asserttrue" | "assertfalse" if !args.is_empty() => {
            let inner = unparen(args[0]);
            let pick = |yes: &str, no: &str| {
                if is_true {
                    yes.to_string()
                } else {
                    no.to_string()
                }
            };
            match call_name(inner, src).as_deref() {
                Some("array_key_exists") => {
                    let a = call_args(inner).filter(|a| a.len() == 2)?;
                    let m = pick("assertArrayHasKey", "assertArrayNotHasKey");
                    rewrite(
                        Rule::AssertArrayHasKey,
                        &m,
                        vec![src_of(a[0]), src_of(a[1])],
                    )
                }
                Some("empty") => {
                    let a = call_args(inner).filter(|a| a.len() == 1)?;
                    let m = pick("assertEmpty", "assertNotEmpty");
                    rewrite(Rule::AssertEmpty, &m, vec![src_of(a[0])])
                }
                Some("in_array") => {
                    let a = call_args(inner).filter(|a| a.len() == 2)?;
                    let m = pick("assertContains", "assertNotContains");
                    rewrite(Rule::AssertContains, &m, vec![src_of(a[0]), src_of(a[1])])
                }
                _ if inner.kind() == "binary_expression"
                    && inner
                        .child_by_field_name("operator")
                        .is_some_and(|o| text(o, src).eq_ignore_ascii_case("instanceof")) =>
                {
                    let subject = inner.child_by_field_name("left")?;
                    let class = inner.child_by_field_name("right")?;
                    if class.kind() != "name" && class.kind() != "qualified_name" {
                        return None;
                    }
                    let m = pick("assertInstanceOf", "assertNotInstanceOf");
                    rewrite(
                        Rule::AssertInstanceOf,
                        &m,
                        vec![format!("{}::class", text(class, src)), src_of(subject)],
                    )
                }
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn rules(src: &str) -> Vec<(Rule, String)> {
        let tree = parse(src);
        let mut out = Findings::default();
        phpunit_checks(tree.root_node(), src, &mut out);
        out.list.into_iter().map(|f| (f.rule, f.detail)).collect()
    }

    #[test]
    fn assert_equals_true_should_be_assert_true() {
        assert_eq!(
            rules("<?php $this->assertEquals(true, $x);"),
            [(Rule::AssertEqualsLiteral, "assertTrue".to_string())]
        );
    }

    #[test]
    fn assert_same_count_should_be_assert_count() {
        assert_eq!(
            rules("<?php $this->assertSame(3, count($x));"),
            [(Rule::AssertCount, "assertCount".to_string())]
        );
    }

    #[test]
    fn assert_false_empty_should_be_assert_not_empty() {
        assert_eq!(
            rules("<?php self::assertFalse(empty($x));"),
            [(Rule::AssertEmpty, "assertNotEmpty".to_string())]
        );
    }

    #[test]
    fn assert_true_instanceof_should_be_assert_instance_of() {
        assert_eq!(
            rules("<?php $this->assertTrue($x instanceof Foo);"),
            [(Rule::AssertInstanceOf, "assertInstanceOf".to_string())]
        );
    }

    #[test]
    fn literal_as_actual_should_be_flagged() {
        assert_eq!(
            rules("<?php $this->assertSame($user->name, 'Ann');"),
            [(Rule::AssertArgumentsOrder, "assertSame".to_string())]
        );
    }

    #[test]
    fn other_receivers_should_be_ignored() {
        assert_eq!(rules("<?php $foo->assertTrue(empty($x));"), []);
    }
}
