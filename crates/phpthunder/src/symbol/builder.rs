//! Stub building: the per-file declaration summary that feeds the index.
//!
//! A compact description of every declaration in a file, enough to answer
//! "where is X declared" and "what does X return" without reparsing the file.
//! Types come from native declarations first, then from PHPDoc.

use super::*;
use crate::syntax::LineIndex;
use crate::syntax::{NameContext, for_each_statement, is_builtin_type, is_class_like, text};
use tree_sitter::Node;

/// Builds stubs for every declaration in a parsed file.
///
/// Covers classes with their members (including PHPDoc virtual members and
/// promoted constructor properties), functions, and global constants, also
/// inside braced namespaces and `if (!function_exists(...))` guards. A class
/// always precedes its members. Pure function of the source; called by
/// [`crate::index::index_source`].
pub fn build(root: Node, src: &str, lines: &LineIndex) -> Vec<Symbol> {
    let mut b = Builder {
        src,
        lines,
        out: Vec::new(),
        templates: Vec::new(),
    };
    for_each_statement(root, src, |stmt, ctx| b.statement(stmt, ctx));
    b.out
}

/// Resolves a native type node; `class` is `None` for builtin types.
///
/// For unions and nullable types the first class wins. `owner` resolves `self`;
/// `static` becomes [`STATIC_TYPE`].
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
                    ..Ty::default()
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

/// Parsed docblock directly above `node`; `None` without one or when the comment is not a `/** */` docblock.
pub fn doc_of(node: Node, src: &str) -> Option<DocBlock> {
    let comment = doc_comment(node)?;
    let raw = text(comment, src);
    is_doc_comment(raw).then(|| parse_docblock(raw))
}

/// True when `node` has a direct child (named or not) of `kind`, e.g. `static_modifier`.
fn has_child(node: Node, kind: &str) -> bool {
    let mut c = node.walk();

    node.children(&mut c).any(|n| n.kind() == kind)
}

/// Accumulates the stubs of one file while [`build`] walks it.
struct Builder<'a> {
    /// Source of the file being walked.
    src: &'a str,
    /// Line index of `src`, for spans.
    lines: &'a LineIndex,
    /// Stubs built so far, in source order.
    out: Vec<Symbol>,
    /// `@template` names in scope (enclosing class, then method), so PHPDoc
    /// types naming them become template markers.
    templates: Vec<String>,
}

impl Builder<'_> {
    /// Declaration header shown on hover: source from `node` up to its body,
    /// whitespace collapsed, trailing `{`/`;` dropped, cut at 240 bytes with `…`.
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

    /// Appends a stub for `decl` named by `name` (`$` stripped), filling spans,
    /// signature, and PHPDoc summary. Returns it so callers can set kind-specific
    /// fields such as the type or `is_static`.
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
            templates: Vec::new(),
            super_args: Vec::new(),
            class_string_params: Vec::new(),
        };
        let index = self.out.len();
        self.out.push(symbol);
        &mut self.out[index]
    }

    /// Native return type, completed by `@return`.
    fn return_type(&self, decl: Node, ctx: &NameContext, owner: Option<&str>) -> Ty {
        let native = decl
            .child_by_field_name("return_type")
            .map(|t| resolve_type(t, self.src, ctx, owner))
            .unwrap_or_default();
        let doc = doc_of(decl, self.src)
            .and_then(|d| {
                d.return_type()
                    .map(|t| resolve_doc_type_in(t, ctx, owner, &self.templates))
            })
            .unwrap_or_default();
        native.or(doc)
    }

    /// Return type, own `@template` names and `class-string<T>` parameters of
    /// a function or method. The callable's templates are in scope (on top
    /// of the class's) while its return type is resolved.
    fn callable_types(
        &mut self,
        decl: Node,
        ctx: &NameContext,
        owner: Option<&str>,
    ) -> (Ty, Vec<String>, Vec<(usize, String)>) {
        let doc = doc_of(decl, self.src);
        let own = doc.as_ref().map(|d| d.templates()).unwrap_or_default();
        let outer = self.templates.len();
        self.templates.extend(own.iter().cloned());
        let ty = self.return_type(decl, ctx, owner);
        self.templates.truncate(outer);
        let mut class_strings = Vec::new();
        if let (Some(doc), Some(params)) = (&doc, decl.child_by_field_name("parameters")) {
            let mut c = params.walk();
            for (i, p) in params.named_children(&mut c).enumerate() {
                let Some(name) = p.child_by_field_name("name") else {
                    continue;
                };
                if let Some(t) =
                    doc.class_string_param(text(name, self.src).trim_start_matches('$'))
                {
                    class_strings.push((i, t));
                }
            }
        }
        (ty, own, class_strings)
    }

    /// Records the declarations of one top-level statement, recursing into conditional blocks so guarded declarations are indexed too.
    fn statement(&mut self, node: Node, ctx: &NameContext) {
        match node.kind() {
            k if is_class_like(k) => self.class_like(node, ctx),
            "function_definition" => {
                let Some(name) = node.child_by_field_name("name") else {
                    return;
                };
                let fqn = ctx.qualify(text(name, self.src));
                let (ty, templates, class_strings) = self.callable_types(node, ctx, None);
                let sym = self.push(Kind::Function, node, name, fqn);
                sym.ty = ty;
                sym.templates = templates;
                sym.class_string_params = class_strings;
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

    /// Records a named class-like with its `extends`/`implements` supertypes, then
    /// its PHPDoc virtual members and body members. Anonymous classes are skipped.
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
        let doc = doc_of(node, self.src);
        let templates = doc.as_ref().map(|d| d.templates()).unwrap_or_default();
        let super_args: Vec<(String, Vec<String>)> = doc
            .iter()
            .flat_map(|d| d.tags.iter())
            .filter_map(|t| match &t.tag {
                Tag::Extends { ty } => {
                    let resolved = resolve_doc_type_in(ty, ctx, Some(&fqn), &templates);
                    Some((resolved.class?, resolved.args))
                }
                _ => None,
            })
            .collect();
        let class_index = self.out.len();
        {
            let sym = self.push(kind, node, name, fqn.clone());
            sym.extends = extends;
            sym.supers = supers;
            sym.templates = templates.clone();
            sym.super_args = super_args;
        }
        // Members see the class's templates; nested classes do not leak them.
        let outer = std::mem::replace(&mut self.templates, templates);
        self.virtual_members(node, ctx, &fqn, class_index);
        if let Some(body) = node.child_by_field_name("body") {
            let mut c = body.walk();
            for member in body.named_children(&mut c) {
                self.member(member, ctx, &fqn, class_index);
            }
        }
        self.templates = outer;
    }

    /// `@property`, `@method`, and `@mixin` from the class docblock.
    fn virtual_members(&mut self, class: Node, ctx: &NameContext, owner: &str, class_index: usize) {
        let Some(comment) = doc_comment(class) else {
            return;
        };
        let raw = text(comment, self.src);
        if !is_doc_comment(raw) {
            return;
        }
        for t in parse_docblock(raw).tags {
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
                    .map(|t| resolve_doc_type_in(t, ctx, Some(owner), &self.templates))
                    .unwrap_or_default(),
                extends: None,
                supers: Vec::new(),
                is_virtual: true,
                templates: Vec::new(),
                super_args: Vec::new(),
                class_string_params: Vec::new(),
            });
        }
    }

    /// Records one class body member: methods (plus promoted constructor
    /// properties), properties, constants, and enum cases. A trait `use` instead
    /// adds the traits to the supertypes of the class at `class_index`.
    fn member(&mut self, node: Node, ctx: &NameContext, owner: &str, class_index: usize) {
        let src = self.src;
        let is_static = has_child(node, "static_modifier");
        match node.kind() {
            "method_declaration" => {
                let Some(name) = node.child_by_field_name("name") else {
                    return;
                };
                let (ty, templates, class_strings) = self.callable_types(node, ctx, Some(owner));
                let sym = self.push(Kind::Method, node, name, owner.to_string());
                sym.is_static = is_static;
                sym.ty = ty;
                sym.templates = templates;
                sym.class_string_params = class_strings;
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
                            .map(|t| resolve_doc_type_in(t, ctx, Some(owner), &self.templates))
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
                        .map(|t| resolve_doc_type_in(t, ctx, Some(owner), &self.templates))
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
                elem: Some("App\\Comment".into()),
                ..Ty::default()
            }
        );
        assert_eq!(
            find(Kind::Property, "tags").ty,
            Ty {
                class: Some("Illuminate\\Support\\Collection".into()),
                elem: Some("App\\Tag".into()),
                args: vec![String::new(), "App\\Tag".into()],
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
