//! Stub building: the per-file declaration summary that feeds the index.
//!
//! A compact description of every declaration in a file, enough to answer
//! "where is X declared" and "what does X return" without reparsing the file.

use crate::syntax::{for_each_statement, is_builtin_type, is_class_like, text, NameContext};
use crate::text::{LineIndex, Span};
use tree_sitter::Node;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Class,
    Interface,
    Trait,
    Enum,
    Function,
    Constant,
    Method,
    Property,
    ClassConstant,
    EnumCase,
}

impl Kind {
    pub fn is_class_like(self) -> bool {
        matches!(self, Kind::Class | Kind::Interface | Kind::Trait | Kind::Enum)
    }

    pub fn is_member(self) -> bool {
        matches!(self, Kind::Method | Kind::Property | Kind::ClassConstant | Kind::EnumCase)
    }
}

/// Members are looked up per namespace: methods, properties, and constants
/// (enum cases share the constant namespace, as in PHP).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemberTag {
    Method,
    Property,
    Constant,
}

impl MemberTag {
    pub fn of(kind: Kind) -> Option<Self> {
        match kind {
            Kind::Method => Some(Self::Method),
            Kind::Property => Some(Self::Property),
            Kind::ClassConstant | Kind::EnumCase => Some(Self::Constant),
            _ => None,
        }
    }
}

/// Return/property type sentinel meaning "late static binding": `static` or `$this`.
pub const STATIC_TYPE: &str = "static";

#[derive(Clone, Debug)]
pub struct Symbol {
    pub kind: Kind,
    /// Declared name (properties without `$`).
    pub name: String,
    /// Fully qualified name for top-level symbols; owner FQN for members.
    pub fqn: String,
    pub span: Span,
    pub name_span: Span,
    pub name_start: usize,
    pub is_static: bool,
    /// Declaration header, e.g. `public static function create(string $name): self`.
    pub signature: String,
    /// Resolved class FQN of the return type (functions/methods) or declared
    /// type (properties); [`STATIC_TYPE`] for `static`/`$this`.
    pub ty: Option<String>,
    /// Parent class (`extends`) for classes.
    pub extends: Option<String>,
    /// Every supertype: parent class, interfaces, used traits.
    pub supers: Vec<String>,
}

impl Symbol {
    pub fn owner(&self) -> Option<&str> {
        self.kind.is_member().then_some(self.fqn.as_str())
    }
}

struct Builder<'a> {
    src: &'a str,
    lines: &'a LineIndex,
    out: Vec<Symbol>,
}

pub fn build(root: Node, src: &str, lines: &LineIndex) -> Vec<Symbol> {
    let mut b = Builder { src, lines, out: Vec::new() };
    for_each_statement(root, src, |stmt, ctx| b.statement(stmt, ctx));
    b.out
}

/// Resolves a type node to a class FQN; `None` for builtin/primitive types.
pub fn resolve_type(node: Node, src: &str, ctx: &NameContext, owner: Option<&str>) -> Option<String> {
    match node.kind() {
        "optional_type" | "named_type" | "union_type" | "intersection_type" | "disjunctive_normal_form_type" => {
            let mut c = node.walk();
            let found = node
                .named_children(&mut c)
                .find_map(|child| resolve_type(child, src, ctx, owner));
            found
        }
        "name" | "qualified_name" | "relative_name" => {
            let raw = text(node, src);
            match raw.to_ascii_lowercase().as_str() {
                "self" => owner.map(str::to_string),
                "static" | "$this" => Some(STATIC_TYPE.to_string()),
                _ if is_builtin_type(raw) => None,
                _ => Some(ctx.resolve_class(raw)),
            }
        }
        _ => None,
    }
}

fn has_child(node: Node, kind: &str) -> bool {
    let mut c = node.walk();
    let found = node.children(&mut c).any(|n| n.kind() == kind);
    found
}

impl Builder<'_> {
    fn header(&self, node: Node) -> String {
        let end = node
            .child_by_field_name("body")
            .map(|b| b.start_byte())
            .unwrap_or(node.end_byte());
        let raw = &self.src[node.start_byte()..end];
        let raw = raw.trim_end_matches([';', '{', ' ', '\n', '\r', '\t']);
        let mut s: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if s.len() > 240 {
            let cut = (0..=240).rev().find(|&i| s.is_char_boundary(i)).unwrap_or(0);
            s.truncate(cut);
            s.push('…');
        }
        s
    }

    fn push(&mut self, kind: Kind, decl: Node, name: Node, fqn: String) -> &mut Symbol {
        let name_text = text(name, self.src).trim_start_matches('$').to_string();
        let symbol = Symbol {
            kind,
            name: name_text,
            fqn,
            span: self.lines.span(self.src, decl.start_byte(), decl.end_byte()),
            name_span: self.lines.span(self.src, name.start_byte(), name.end_byte()),
            name_start: name.start_byte(),
            is_static: false,
            signature: self.header(decl),
            ty: None,
            extends: None,
            supers: Vec::new(),
        };
        self.out.push(symbol);
        self.out.last_mut().unwrap()
    }

    fn statement(&mut self, node: Node, ctx: &NameContext) {
        match node.kind() {
            k if is_class_like(k) => self.class_like(node, ctx),
            "function_definition" => {
                let Some(name) = node.child_by_field_name("name") else { return };
                let fqn = ctx.qualify(text(name, self.src));
                let ty = node
                    .child_by_field_name("return_type")
                    .and_then(|t| resolve_type(t, self.src, ctx, None));
                self.push(Kind::Function, node, name, fqn).ty = ty;
            }
            "const_declaration" => {
                let mut c = node.walk();
                for el in node.named_children(&mut c).filter(|n| n.kind() == "const_element") {
                    if let Some(name) = el.named_child(0).filter(|n| n.kind() == "name") {
                        let fqn = ctx.qualify(text(name, self.src));
                        self.push(Kind::Constant, el, name, fqn);
                    }
                }
            }
            // Conditional declarations: `if (!function_exists('x')) { function x() {} }`.
            "if_statement" | "compound_statement" | "else_clause" | "else_if_clause"
            | "colon_block" => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    self.statement(child, ctx);
                }
            }
            _ => {}
        }
    }

    fn class_like(&mut self, node: Node, ctx: &NameContext) {
        let Some(name) = node.child_by_field_name("name") else { return };
        let fqn = ctx.qualify(text(name, self.src));
        let kind = match node.kind() {
            "interface_declaration" => Kind::Interface,
            "trait_declaration" => Kind::Trait,
            "enum_declaration" => Kind::Enum,
            _ => Kind::Class,
        };
        let mut extends = None;
        let mut supers = Vec::new();
        let mut c = node.walk();
        for clause in node.named_children(&mut c) {
            if !matches!(clause.kind(), "base_clause" | "class_interface_clause") {
                continue;
            }
            let mut cc = clause.walk();
            for n in clause.named_children(&mut cc) {
                if matches!(n.kind(), "name" | "qualified_name" | "relative_name") {
                    let target = ctx.resolve_class(text(n, self.src));
                    if clause.kind() == "base_clause" && kind == Kind::Class && extends.is_none() {
                        extends = Some(target.clone());
                    }
                    supers.push(target);
                }
            }
        }
        let class_index = self.out.len();
        {
            let sym = self.push(kind, node, name, fqn.clone());
            sym.extends = extends;
            sym.supers = supers;
        }
        let Some(body) = node.child_by_field_name("body") else { return };
        let mut c = body.walk();
        for member in body.named_children(&mut c) {
            self.member(member, ctx, &fqn, class_index);
        }
    }

    fn member(&mut self, node: Node, ctx: &NameContext, owner: &str, class_index: usize) {
        let src = self.src;
        let is_static = has_child(node, "static_modifier");
        match node.kind() {
            "method_declaration" => {
                let Some(name) = node.child_by_field_name("name") else { return };
                let ty = node
                    .child_by_field_name("return_type")
                    .and_then(|t| resolve_type(t, src, ctx, Some(owner)));
                let sym = self.push(Kind::Method, node, name, owner.to_string());
                sym.is_static = is_static;
                sym.ty = ty;
                // Constructor property promotion: `__construct(private Foo $foo)`.
                if text(name, src).eq_ignore_ascii_case("__construct") {
                    if let Some(params) = node.child_by_field_name("parameters") {
                        let mut c = params.walk();
                        for p in params
                            .named_children(&mut c)
                            .filter(|p| p.kind() == "property_promotion_parameter")
                        {
                            let Some(var) = p.child_by_field_name("name") else { continue };
                            let ty = p
                                .child_by_field_name("type")
                                .and_then(|t| resolve_type(t, src, ctx, Some(owner)));
                            self.push(Kind::Property, p, var, owner.to_string()).ty = ty;
                        }
                    }
                }
            }
            "property_declaration" => {
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|t| resolve_type(t, src, ctx, Some(owner)));
                let mut c = node.walk();
                for el in node.named_children(&mut c).filter(|n| n.kind() == "property_element") {
                    let Some(var) = el.child_by_field_name("name") else { continue };
                    let sym = self.push(Kind::Property, node, var, owner.to_string());
                    sym.is_static = is_static;
                    sym.ty = ty.clone();
                }
            }
            "const_declaration" => {
                let mut c = node.walk();
                for el in node.named_children(&mut c).filter(|n| n.kind() == "const_element") {
                    if let Some(name) = el.named_child(0).filter(|n| n.kind() == "name") {
                        self.push(Kind::ClassConstant, node, name, owner.to_string()).is_static = true;
                    }
                }
            }
            "enum_case" => {
                let Some(name) = node.child_by_field_name("name") else { return };
                let sym = self.push(Kind::EnumCase, node, name, owner.to_string());
                sym.is_static = true;
                sym.ty = Some(owner.to_string());
            }
            "use_declaration" => {
                let mut c = node.walk();
                let traits: Vec<String> = node
                    .named_children(&mut c)
                    .filter(|n| matches!(n.kind(), "name" | "qualified_name" | "relative_name"))
                    .map(|n| ctx.resolve_class(text(n, src)))
                    .collect();
                self.out[class_index].supers.extend(traits);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    #[test]
    fn builds_stubs_for_declarations() {
        let src = r#"<?php
namespace App\Models;
use App\Contracts\HasName;
final class User extends Model implements HasName {
    use Greets;
    public const ROLE = 'user';
    private ?Address $address;
    public function __construct(private Repo $repo) {}
    public static function create(string $name): static { return new static(); }
}
enum Status: string { case Active = 'a'; }
if (!function_exists('helper')) { function helper(): User {} }
"#;
        let tree = parse(src);
        let li = LineIndex::new(src);
        let syms = build(tree.root_node(), src, &li);
        let find = |k: Kind, n: &str| syms.iter().find(|s| s.kind == k && s.name == n).unwrap();

        let user = find(Kind::Class, "User");
        assert_eq!(user.fqn, "App\\Models\\User");
        assert_eq!(user.extends.as_deref(), Some("App\\Models\\Model"));
        assert_eq!(user.supers, ["App\\Models\\Model", "App\\Contracts\\HasName", "App\\Models\\Greets"]);

        assert_eq!(find(Kind::Property, "address").ty.as_deref(), Some("App\\Models\\Address"));
        assert_eq!(find(Kind::Property, "repo").ty.as_deref(), Some("App\\Models\\Repo"));
        let create = find(Kind::Method, "create");
        assert!(create.is_static);
        assert_eq!(create.ty.as_deref(), Some(STATIC_TYPE));
        assert_eq!(create.signature, "public static function create(string $name): static");
        assert_eq!(find(Kind::ClassConstant, "ROLE").fqn, "App\\Models\\User");
        assert_eq!(find(Kind::EnumCase, "Active").fqn, "App\\Models\\Status");
        assert_eq!(find(Kind::Function, "helper").fqn, "App\\Models\\helper");
    }
}
