//! Stub building: the per-file declaration summary that feeds the index.
//!
//! A compact description of every declaration in a file, enough to answer
//! "where is X declared" and "what does X return" without reparsing the file.
//! Types come from native declarations first, then from PHPDoc.

use crate::phpdoc::{self, DocBlock, Tag, Ty};
use crate::syntax::{NameContext, for_each_statement, is_builtin_type, is_class_like, text};
use crate::text::{LineIndex, Span};
use tree_sitter::Node;

pub use crate::phpdoc::STATIC_TYPE;

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
        matches!(
            self,
            Kind::Class | Kind::Interface | Kind::Trait | Kind::Enum
        )
    }

    pub fn is_member(self) -> bool {
        matches!(
            self,
            Kind::Method | Kind::Property | Kind::ClassConstant | Kind::EnumCase
        )
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
    /// First paragraph of the PHPDoc comment.
    pub doc: String,
    /// Return type (functions/methods) or declared type (properties).
    pub ty: Ty,
    /// Parent class (`extends`) for classes.
    pub extends: Option<String>,
    /// Every supertype: parent class, interfaces, used traits, `@mixin` classes.
    pub supers: Vec<String>,
    /// Declared only in PHPDoc (`@property`, `@method`).
    pub is_virtual: bool,
}

pub fn build(root: Node, src: &str, lines: &LineIndex) -> Vec<Symbol> {
    let mut b = Builder {
        src,
        lines,
        out: Vec::new(),
    };
    for_each_statement(root, src, |stmt, ctx| b.statement(stmt, ctx));
    b.out
}

/// Resolves a native type node; `class` is `None` for builtin types.
pub fn resolve_type(node: Node, src: &str, ctx: &NameContext, owner: Option<&str>) -> Ty {
    match node.kind() {
        "optional_type"
        | "named_type"
        | "union_type"
        | "intersection_type"
        | "disjunctive_normal_form_type" => {
            let mut c = node.walk();

            node.named_children(&mut c)
                .map(|child| resolve_type(child, src, ctx, owner))
                .find(|t| !t.is_empty())
                .unwrap_or_default()
        }
        "name" | "qualified_name" | "relative_name" => {
            let raw = text(node, src);
            match raw.to_ascii_lowercase().as_str() {
                "self" => Ty {
                    class: owner.map(str::to_string),
                    elem: None,
                },
                "static" | "$this" => Ty::class(STATIC_TYPE),
                _ if is_builtin_type(raw) => Ty::default(),
                _ => Ty::class(ctx.resolve_class(raw)),
            }
        }
        _ => Ty::default(),
    }
}

/// The `/** ... */` comment directly above a declaration or statement.
pub fn doc_comment(node: Node) -> Option<Node> {
    let prev = node.prev_named_sibling()?;
    (prev.kind() == "comment").then_some(prev)
}

pub fn doc_of(node: Node, src: &str) -> Option<DocBlock> {
    let comment = doc_comment(node)?;
    let raw = text(comment, src);
    phpdoc::is_doc_comment(raw).then(|| phpdoc::parse(raw))
}

fn has_child(node: Node, kind: &str) -> bool {
    let mut c = node.walk();

    node.children(&mut c).any(|n| n.kind() == kind)
}

struct Builder<'a> {
    src: &'a str,
    lines: &'a LineIndex,
    out: Vec<Symbol>,
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
            let cut = (0..=240)
                .rev()
                .find(|&i| s.is_char_boundary(i))
                .unwrap_or(0);
            s.truncate(cut);
            s.push('…');
        }
        s
    }

    fn push(&mut self, kind: Kind, decl: Node, name: Node, fqn: String) -> &mut Symbol {
        let doc = doc_of(decl, self.src)
            .map(|d| d.summary)
            .unwrap_or_default();
        let symbol = Symbol {
            kind,
            name: text(name, self.src).trim_start_matches('$').to_string(),
            fqn,
            span: self
                .lines
                .span(self.src, decl.start_byte(), decl.end_byte()),
            name_span: self
                .lines
                .span(self.src, name.start_byte(), name.end_byte()),
            name_start: name.start_byte(),
            is_static: false,
            signature: self.header(decl),
            doc,
            ty: Ty::default(),
            extends: None,
            supers: Vec::new(),
            is_virtual: false,
        };
        self.out.push(symbol);
        self.out.last_mut().unwrap()
    }

    /// Native return type, completed by `@return`.
    fn return_type(&self, decl: Node, ctx: &NameContext, owner: Option<&str>) -> Ty {
        let native = decl
            .child_by_field_name("return_type")
            .map(|t| resolve_type(t, self.src, ctx, owner))
            .unwrap_or_default();
        let doc = doc_of(decl, self.src)
            .and_then(|d| d.return_type().map(|t| phpdoc::resolve(t, ctx, owner)))
            .unwrap_or_default();
        native.or(doc)
    }

    fn statement(&mut self, node: Node, ctx: &NameContext) {
        match node.kind() {
            k if is_class_like(k) => self.class_like(node, ctx),
            "function_definition" => {
                let Some(name) = node.child_by_field_name("name") else {
                    return;
                };
                let fqn = ctx.qualify(text(name, self.src));
                let ty = self.return_type(node, ctx, None);
                self.push(Kind::Function, node, name, fqn).ty = ty;
            }
            "const_declaration" => {
                let mut c = node.walk();
                for el in node
                    .named_children(&mut c)
                    .filter(|n| n.kind() == "const_element")
                {
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
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
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
        self.virtual_members(node, ctx, &fqn, class_index);
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let mut c = body.walk();
        for member in body.named_children(&mut c) {
            self.member(member, ctx, &fqn, class_index);
        }
    }

    /// `@property`, `@method`, and `@mixin` from the class docblock.
    fn virtual_members(&mut self, class: Node, ctx: &NameContext, owner: &str, class_index: usize) {
        let Some(comment) = doc_comment(class) else {
            return;
        };
        let raw = text(comment, self.src);
        if !phpdoc::is_doc_comment(raw) {
            return;
        }
        for t in phpdoc::parse(raw).tags {
            let (kind, name, ty, is_static) = match &t.tag {
                Tag::Property { ty, name } => (Kind::Property, name, ty, false),
                Tag::Method {
                    is_static,
                    ty,
                    name,
                } => (Kind::Method, name, ty, *is_static),
                Tag::Mixin { class } => {
                    self.out[class_index].supers.push(ctx.resolve_class(class));
                    continue;
                }
                _ => continue,
            };
            let start = comment.start_byte() + t.offset;
            let end = start + t.line.len().min(raw.len() - t.offset);
            self.out.push(Symbol {
                kind,
                name: name.clone(),
                fqn: owner.to_string(),
                span: self.lines.span(self.src, start, end),
                name_span: self.lines.span(self.src, start, end),
                name_start: start,
                is_static,
                signature: t.line.clone(),
                doc: String::new(),
                ty: ty
                    .as_deref()
                    .map(|t| phpdoc::resolve(t, ctx, Some(owner)))
                    .unwrap_or_default(),
                extends: None,
                supers: Vec::new(),
                is_virtual: true,
            });
        }
    }

    fn member(&mut self, node: Node, ctx: &NameContext, owner: &str, class_index: usize) {
        let src = self.src;
        let is_static = has_child(node, "static_modifier");
        match node.kind() {
            "method_declaration" => {
                let Some(name) = node.child_by_field_name("name") else {
                    return;
                };
                let ty = self.return_type(node, ctx, Some(owner));
                let sym = self.push(Kind::Method, node, name, owner.to_string());
                sym.is_static = is_static;
                sym.ty = ty;
                // Constructor property promotion: `__construct(private Foo $foo)`.
                if text(name, src).eq_ignore_ascii_case("__construct") {
                    let doc = doc_of(node, src);
                    let Some(params) = node.child_by_field_name("parameters") else {
                        return;
                    };
                    let mut c = params.walk();
                    for p in params
                        .named_children(&mut c)
                        .filter(|p| p.kind() == "property_promotion_parameter")
                    {
                        let Some(var) = p.child_by_field_name("name") else {
                            continue;
                        };
                        let native = p
                            .child_by_field_name("type")
                            .map(|t| resolve_type(t, src, ctx, Some(owner)))
                            .unwrap_or_default();
                        let documented = doc
                            .as_ref()
                            .and_then(|d| d.param_type(text(var, src).trim_start_matches('$')))
                            .map(|t| phpdoc::resolve(t, ctx, Some(owner)))
                            .unwrap_or_default();
                        self.push(Kind::Property, p, var, owner.to_string()).ty =
                            native.or(documented);
                    }
                }
            }
            "property_declaration" => {
                let native = node
                    .child_by_field_name("type")
                    .map(|t| resolve_type(t, src, ctx, Some(owner)))
                    .unwrap_or_default();
                let doc = doc_of(node, src);
                let mut c = node.walk();
                for el in node
                    .named_children(&mut c)
                    .filter(|n| n.kind() == "property_element")
                {
                    let Some(var) = el.child_by_field_name("name") else {
                        continue;
                    };
                    let name = text(var, src).trim_start_matches('$');
                    let documented = doc
                        .as_ref()
                        .and_then(|d| d.var_type(Some(name)))
                        .map(|t| phpdoc::resolve(t, ctx, Some(owner)))
                        .unwrap_or_default();
                    let sym = self.push(Kind::Property, node, var, owner.to_string());
                    sym.is_static = is_static;
                    sym.ty = native.clone().or(documented);
                }
            }
            "const_declaration" => {
                let mut c = node.walk();
                for el in node
                    .named_children(&mut c)
                    .filter(|n| n.kind() == "const_element")
                {
                    if let Some(name) = el.named_child(0).filter(|n| n.kind() == "name") {
                        self.push(Kind::ClassConstant, node, name, owner.to_string())
                            .is_static = true;
                    }
                }
            }
            "enum_case" => {
                let Some(name) = node.child_by_field_name("name") else {
                    return;
                };
                let sym = self.push(Kind::EnumCase, node, name, owner.to_string());
                sym.is_static = true;
                sym.ty = Ty::class(owner);
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

    fn stubs(src: &str) -> Vec<Symbol> {
        let tree = parse(src);
        build(tree.root_node(), src, &LineIndex::new(src))
    }

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
        let syms = stubs(src);
        let find = |k: Kind, n: &str| syms.iter().find(|s| s.kind == k && s.name == n).unwrap();

        let user = find(Kind::Class, "User");
        assert_eq!(user.fqn, "App\\Models\\User");
        assert_eq!(user.extends.as_deref(), Some("App\\Models\\Model"));
        assert_eq!(
            user.supers,
            [
                "App\\Models\\Model",
                "App\\Contracts\\HasName",
                "App\\Models\\Greets"
            ]
        );

        assert_eq!(
            find(Kind::Property, "address").ty,
            Ty::class("App\\Models\\Address")
        );
        assert_eq!(
            find(Kind::Property, "repo").ty,
            Ty::class("App\\Models\\Repo")
        );
        let create = find(Kind::Method, "create");
        assert!(create.is_static);
        assert_eq!(create.ty, Ty::class(STATIC_TYPE));
        assert_eq!(
            create.signature,
            "public static function create(string $name): static"
        );
        assert_eq!(find(Kind::ClassConstant, "ROLE").fqn, "App\\Models\\User");
        assert_eq!(find(Kind::EnumCase, "Active").fqn, "App\\Models\\Status");
        assert_eq!(find(Kind::Function, "helper").fqn, "App\\Models\\helper");
    }

    #[test]
    fn reads_types_from_phpdoc() {
        let src = r#"<?php
namespace App;
use Illuminate\Support\Collection;
/**
 * A blog post.
 * @property-read Author $author
 * @method static Builder published()
 * @mixin \Illuminate\Database\Eloquent\Builder
 */
class Post {
    /** @var Comment[] */
    public $comments;
    /** @var Collection<int, Tag> */
    private Collection $tags;
    /**
     * Latest post.
     * @return static|null
     */
    public static function latest() {}
    /** @return Comment[] */
    public function approved(): array {}
}
"#;
        let syms = stubs(src);
        let find = |k: Kind, n: &str| syms.iter().find(|s| s.kind == k && s.name == n).unwrap();
        let post = find(Kind::Class, "Post");
        assert_eq!(post.doc, "A blog post.");
        assert_eq!(post.supers, ["Illuminate\\Database\\Eloquent\\Builder"]);
        let author = find(Kind::Property, "author");
        assert!(author.is_virtual);
        assert_eq!(author.ty, Ty::class("App\\Author"));
        assert_eq!(author.name_span.start.line, 5);
        let published = find(Kind::Method, "published");
        assert!(published.is_static && published.is_virtual);
        assert_eq!(published.ty, Ty::class("App\\Builder"));
        assert_eq!(
            find(Kind::Property, "comments").ty,
            Ty {
                class: None,
                elem: Some("App\\Comment".into())
            }
        );
        assert_eq!(
            find(Kind::Property, "tags").ty,
            Ty {
                class: Some("Illuminate\\Support\\Collection".into()),
                elem: Some("App\\Tag".into())
            }
        );
        let latest = find(Kind::Method, "latest");
        assert_eq!(latest.ty, Ty::class(STATIC_TYPE));
        assert_eq!(latest.doc, "Latest post.");
        assert_eq!(
            find(Kind::Method, "approved").ty.elem.as_deref(),
            Some("App\\Comment")
        );
    }
}
