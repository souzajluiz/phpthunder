//! Parameter info for the call around the caret.

use super::*;
use crate::analysis::{Analyzer, Target, resolve};
use crate::index::Index;
use crate::symbol::{MemberTag, Symbol};
use crate::syntax::{parse, text};
use tree_sitter::Node;

/// Splits `a, b<c, d>, e` at top-level commas (not inside brackets).
fn split_top_level(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        parts.push(last.to_string());
    }
    parts
}

/// Parameters listed in a declaration header (between its outer parentheses).
fn params_of(signature: &str) -> Vec<String> {
    let Some(open) = signature.find('(') else {
        return Vec::new();
    };
    let mut depth = 0i32;
    let mut close = signature.len();
    for (i, ch) in signature[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    split_top_level(&signature[open + 1..close])
}

/// The callee declaration for a call node, if it resolves.
fn callee(index: &Index, analyzer: &Analyzer, call: Node) -> Option<Symbol> {
    let name = match call.kind() {
        "function_call_expression" => {
            let f = call.child_by_field_name("function")?;
            let mut n = f;
            // `App\fn()`: classify the last segment.
            while n.kind() != "name" {
                n = n.named_child(n.named_child_count().checked_sub(1)? as u32)?;
            }
            n
        }
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            call.child_by_field_name("name")?
        }
        "object_creation_expression" => {
            let mut c = call.walk();
            let class = call
                .named_children(&mut c)
                .find(|n| matches!(n.kind(), "name" | "qualified_name"))?;
            let class_name = if class.kind() == "name" {
                class
            } else {
                class.named_child(class.named_child_count().checked_sub(1)? as u32)?
            };
            let Target::Class(fqn) = analyzer.target_of(class_name)? else {
                return None;
            };
            let ctor = index.find_member(&fqn, MemberTag::Method, "__construct");
            return ctor.first().map(|(_, s)| (*s).clone());
        }
        _ => return None,
    };
    let target = analyzer.target_of(name)?;
    resolve(index, &target).into_iter().next().map(|d| d.symbol)
}

/// Parameter info at `byte`, or `None` when the caret is not inside the
/// argument list of a resolvable call.
pub fn signature_help(index: &Index, src: &str, byte: usize) -> Option<SignatureHelp> {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    let mut node = tree.root_node().descendant_for_byte_range(byte, byte)?;
    // Walk up to the innermost argument list that contains the caret.
    let args = loop {
        if node.kind() == "arguments"
            && node.start_byte() < byte
            && (byte < node.end_byte() || node.has_error() || text(node, src).ends_with('('))
        {
            break node;
        }
        node = node.parent()?;
    };
    let call = args.parent()?;
    let symbol = callee(index, &analyzer, call)?;
    let params = params_of(&symbol.signature);
    let mut commas = 0;
    let mut c = args.walk();
    for child in args.children(&mut c) {
        if child.kind() == "," && child.end_byte() <= byte {
            commas += 1;
        }
    }
    let variadic = params.last().is_some_and(|p| p.contains("..."));
    let active = if variadic {
        commas.min(params.len().saturating_sub(1))
    } else {
        commas
    };
    Some(SignatureHelp {
        label: symbol.signature,
        params,
        active,
    })
}
