//! Reference resolution and type inference over one parsed file.
//!
//! Classifies the identifier under the caret by its syntactic position and
//! resolves it to declarations (possibly several) through the index.

use super::*;
use crate::index::Index;
use crate::symbol::{MemberTag, doc_of, resolve_type};
use crate::symbol::{Tag, Ty, is_doc_comment, parse_docblock, resolve_doc_type};
use crate::syntax::{NameContext, UseKind, context_at, is_class_like, parse, text, use_clauses};
use tree_sitter::{Node, Tree};

/// Looks up every declaration `target` may refer to.
///
/// Returns several when the name is declared more than once (for example the
/// same class in two vendor packages) and an empty vec when nothing is indexed
/// under it. Member targets walk the inheritance chain, nearest declaration
/// first. Used by go to declaration, hover, and find usages.
pub fn resolve(index: &Index, target: &Target) -> Vec<Definition> {
    let hits = match target {
        Target::Class(fqn) => index.classes(fqn),
        Target::Function(candidates) => index.functions(candidates),
        Target::Member { class, tag, name } => index.find_member(class, *tag, name),
    };
    hits.into_iter()
        .map(|(p, s)| Definition {
            path: p.to_path_buf(),
            symbol: s.clone(),
        })
        .collect()
}

/// Recursion cap for [`Analyzer::ty_of`]; guards against self-referencing
/// assignments (`$a = $a->next()`) and pathological fluent chains.
const MAX_INFERENCE_DEPTH: u8 = 8;

/// Per-file analysis session: what names refer to and what types expressions have.
///
/// Borrows one parsed file plus the project index for cross-file lookups. Cheap
/// to build; the server and [`find_references`] create one per file per request.
pub struct Analyzer<'a> {
    /// Project index used for class, function, and member lookups.
    pub index: &'a Index,
    /// Source text that `tree` was parsed from.
    pub src: &'a str,
    /// Syntax tree of `src`.
    pub tree: &'a Tree,
}

/// True for node kinds that spell a class or function name (plain, qualified, or `namespace\`-relative).
pub(crate) fn is_name(kind: &str) -> bool {
    matches!(kind, "name" | "qualified_name" | "relative_name")
}

/// True when the optional field child `a` is exactly node `b`; tells which side of an expression a name sits on.
pub(crate) fn same(a: Option<Node>, b: Node) -> bool {
    a.is_some_and(|a| a.id() == b.id())
}

impl<'a> Analyzer<'a> {
    /// Wraps an already parsed file; does no work itself. `tree` must be `parse(src)`.
    pub fn new(index: &'a Index, src: &'a str, tree: &'a Tree) -> Self {
        Self { index, src, tree }
    }

    /// Source text covered by `node`.
    pub(crate) fn text(&self, node: Node) -> &'a str {
        text(node, self.src)
    }

    /// Namespace and `use` imports in effect at `byte`, needed to qualify names written there. Re-walks the file's top-level statements on each call.
    pub(crate) fn ctx(&self, byte: usize) -> NameContext {
        context_at(self.tree.root_node(), self.src, byte)
    }

    /// The identifier node under the caret (the caret may sit just after it).
    pub fn name_node_at(&self, byte: usize) -> Option<Node<'a>> {
        let root = self.tree.root_node();
        [byte, byte.saturating_sub(1)].into_iter().find_map(|b| {
            let node = root.named_descendant_for_byte_range(b, b)?;
            let node = if node.kind() == "variable_name" {
                node.named_child(0)?
            } else {
                node
            };
            (node.kind() == "name").then_some(node)
        })
    }

    /// What the identifier under the caret refers to, plus the identifier node.
    pub fn target_at(&self, byte: usize) -> Option<(Target, Node<'a>)> {
        let name = self.name_node_at(byte)?;
        Some((self.target_of(name)?, name))
    }

    /// Classifies a `name` node by its syntactic position.
    pub fn target_of(&self, name: Node<'a>) -> Option<Target> {
        // Climb from a segment (`Models` in `App\Models\User`) to the full name.
        let mut node = name;
        if let Some(p) = node.parent().filter(|p| p.kind() == "namespace_name") {
            node = p;
        }
        if let Some(p) = node
            .parent()
            .filter(|p| matches!(p.kind(), "qualified_name" | "relative_name"))
        {
            node = p;
        }
        let parent = node.parent()?;
        let raw = self.text(node);
        let ctx = self.ctx(node.start_byte());
        let member = |class: Option<String>, tag| {
            Some(Target::Member {
                class: class?,
                tag,
                name: raw.to_string(),
            })
        };
        match parent.kind() {
            "member_call_expression" | "nullsafe_member_call_expression"
                if same(parent.child_by_field_name("name"), node) =>
            {
                let object = parent.child_by_field_name("object")?;
                member(self.type_of(object, 0), MemberTag::Method)
            }
            "member_access_expression" | "nullsafe_member_access_expression"
                if same(parent.child_by_field_name("name"), node) =>
            {
                let object = parent.child_by_field_name("object")?;
                member(self.type_of(object, 0), MemberTag::Property)
            }
            "scoped_call_expression" if same(parent.child_by_field_name("name"), node) => {
                let scope = parent.child_by_field_name("scope")?;
                member(self.class_of_scope(scope, 0), MemberTag::Method)
            }
            "class_constant_access_expression" => {
                let scope = parent.named_child(0)?;
                if scope.id() == node.id() {
                    return self.class_ref(node, &ctx).map(Target::Class);
                }
                if raw.eq_ignore_ascii_case("class") {
                    return self.class_of_scope(scope, 0).map(Target::Class);
                }
                member(self.class_of_scope(scope, 0), MemberTag::Constant)
            }
            "variable_name" => {
                let holder = parent.parent()?;
                match holder.kind() {
                    "scoped_property_access_expression"
                        if same(holder.child_by_field_name("name"), parent) =>
                    {
                        let scope = holder.child_by_field_name("scope")?;
                        member(self.class_of_scope(scope, 0), MemberTag::Property)
                    }
                    "property_element" | "property_promotion_parameter" => {
                        member(self.enclosing_class(holder, &ctx), MemberTag::Property)
                    }
                    _ => None,
                }
            }
            "function_call_expression" if same(parent.child_by_field_name("function"), node) => {
                Some(Target::Function(ctx.resolve_function(raw)))
            }
            "function_definition" => Some(Target::Function(vec![ctx.qualify(raw)])),
            k if is_class_like(k) => Some(Target::Class(ctx.qualify(raw))),
            "method_declaration" => member(self.enclosing_class(parent, &ctx), MemberTag::Method),
            "const_element" => member(self.enclosing_class(parent, &ctx), MemberTag::Constant),
            "enum_case" => member(self.enclosing_class(parent, &ctx), MemberTag::Constant),
            "namespace_use_clause" => {
                let decl = parent
                    .parent()
                    .filter(|d| d.kind() == "namespace_use_declaration")
                    .or_else(|| parent.parent()?.parent())?;
                let clause = use_clauses(decl, self.src)
                    .into_iter()
                    .find(|c| c.name_node.id() == node.id() || same(c.alias_node, node))?;
                match clause.kind {
                    UseKind::Class => Some(Target::Class(clause.fqn)),
                    UseKind::Function => Some(Target::Function(vec![clause.fqn])),
                    UseKind::Const => None,
                }
            }
            "named_type"
            | "base_clause"
            | "class_interface_clause"
            | "object_creation_expression"
            | "binary_expression"
            | "type_list"
            | "catch_clause"
            | "attribute"
            | "use_declaration"
            | "scoped_call_expression"
            | "scoped_property_access_expression"
            | "optional_type"
            | "union_type"
            | "intersection_type" => self.class_ref(node, &ctx).map(Target::Class),
            _ => None,
        }
    }

    /// Resolves a class name as written, handling `self`, `static`, and `parent`.
    pub(crate) fn class_ref(&self, node: Node, ctx: &NameContext) -> Option<String> {
        let raw = self.text(node);
        match raw.to_ascii_lowercase().as_str() {
            "self" | "static" => self.enclosing_class(node, ctx),
            "parent" => {
                let cls = self.enclosing_class(node, ctx)?;
                self.index
                    .classes(&cls)
                    .into_iter()
                    .find_map(|(_, s)| s.extends.clone())
            }
            _ => Some(ctx.resolve_class(raw)),
        }
    }

    /// FQN of the innermost named class-like declaration around `node`.
    ///
    /// Backs `self`, `static`, `$this`, and member declarations. `None` at top
    /// level and inside anonymous classes (they have no name to resolve).
    pub(crate) fn enclosing_class(&self, node: Node, ctx: &NameContext) -> Option<String> {
        let mut cur = node.parent();
        while let Some(n) = cur {
            if is_class_like(n.kind()) {
                return Some(ctx.qualify(self.text(n.child_by_field_name("name")?)));
            }
            cur = n.parent();
        }
        None
    }

    /// Class referenced by the left side of `::`: a written name (`Foo`, `self`,
    /// `parent`) is resolved directly, any other expression (`$obj::`) by type
    /// inference.
    pub(crate) fn class_of_scope(&self, scope: Node, depth: u8) -> Option<String> {
        if is_name(scope.kind()) {
            self.class_ref(scope, &self.ctx(scope.start_byte()))
        } else {
            self.type_of(scope, depth)
        }
    }

    /// Lightweight type inference: the class an expression evaluates to.
    ///
    /// `depth` is the current recursion depth (callers start at 0). Returns `None`
    /// for scalars, unknown types, and when inference gives up.
    pub fn type_of(&self, expr: Node<'a>, depth: u8) -> Option<String> {
        self.ty_of(expr, depth).class
    }

    /// Full inferred type of an expression, including the element type of
    /// arrays/collections (from PHPDoc) for `foreach` and `$xs[0]`.
    ///
    /// Follows variables, `new`, member and static calls, function calls, and
    /// subscripts; `static` return types are bound to the receiver class. Returns
    /// an empty [`Ty`] once `depth` exceeds `MAX_INFERENCE_DEPTH` (8).
    pub fn ty_of(&self, expr: Node<'a>, depth: u8) -> Ty {
        if depth > MAX_INFERENCE_DEPTH {
            return Ty::default();
        }
        let d = depth + 1;
        let member_ty = |class: Option<String>, tag, name: Option<Node>| -> Ty {
            let (Some(class), Some(name)) = (class, name) else {
                return Ty::default();
            };
            let found = self.index.find_member(&class, tag, self.text(name));
            found
                .first()
                .map(|(_, s)| s.ty.clone().bind_static(&class))
                .unwrap_or_default()
        };
        let child = |field: &str| expr.child_by_field_name(field);
        match expr.kind() {
            "variable_name" => {
                let ctx = self.ctx(expr.start_byte());
                match self.text(expr) {
                    "$this" => Ty {
                        class: self.enclosing_class(expr, &ctx),
                        elem: None,
                    },
                    var => self.variable_type(expr, var, &ctx, d),
                }
            }
            "object_creation_expression" => {
                let mut c = expr.walk();
                let class = expr.named_children(&mut c).find(|n| is_name(n.kind()));
                Ty {
                    class: class.and_then(|n| self.class_ref(n, &self.ctx(n.start_byte()))),
                    elem: None,
                }
            }
            "member_call_expression" | "nullsafe_member_call_expression" => member_ty(
                child("object").and_then(|o| self.type_of(o, d)),
                MemberTag::Method,
                child("name"),
            ),
            "member_access_expression" | "nullsafe_member_access_expression" => member_ty(
                child("object").and_then(|o| self.type_of(o, d)),
                MemberTag::Property,
                child("name"),
            ),
            "scoped_call_expression" => member_ty(
                child("scope").and_then(|s| self.class_of_scope(s, d)),
                MemberTag::Method,
                child("name"),
            ),
            "scoped_property_access_expression" => member_ty(
                child("scope").and_then(|s| self.class_of_scope(s, d)),
                MemberTag::Property,
                child("name").and_then(|n| n.named_child(0)),
            ),
            "class_constant_access_expression" => member_ty(
                expr.named_child(0).and_then(|s| self.class_of_scope(s, d)),
                MemberTag::Constant,
                expr.named_child(1),
            ),
            "function_call_expression" => {
                let Some(f) = child("function") else {
                    return Ty::default();
                };
                let ctx = self.ctx(f.start_byte());
                let found = self.index.functions(&ctx.resolve_function(self.text(f)));
                found.first().map(|(_, s)| s.ty.clone()).unwrap_or_default()
            }
            "subscript_expression" => {
                let elem = expr.named_child(0).and_then(|a| self.ty_of(a, d).elem);
                Ty {
                    class: elem,
                    elem: None,
                }
            }
            "parenthesized_expression" => expr
                .named_child(0)
                .map(|e| self.ty_of(e, d))
                .unwrap_or_default(),
            "assignment_expression" => child("right").map(|e| self.ty_of(e, d)).unwrap_or_default(),
            _ => Ty::default(),
        }
    }

    /// `@var` type in the docblock above the statement containing `node`.
    fn inline_var_doc(&self, node: Node, var: &str, ctx: &NameContext) -> Option<Ty> {
        let stmt = node
            .parent()
            .filter(|p| p.kind() == "expression_statement")
            .unwrap_or(node);
        let doc = doc_of(stmt, self.src)?;
        let ty = doc.var_type(Some(var.trim_start_matches('$')))?;
        let owner = self.enclosing_class(node, ctx);
        Some(resolve_doc_type(ty, ctx, owner.as_deref()))
    }

    /// Type of `$var` at `usage`. The source closest before the usage wins:
    /// an assignment (or its `@var`), a `foreach` binding, a `catch` variable,
    /// or a standalone `/** @var Type $var */`. Parameters (native type, then
    /// `@param`) are the fallback.
    fn variable_type(&self, usage: Node<'a>, var: &str, ctx: &NameContext, depth: u8) -> Ty {
        let scope = enclosing_function(usage).unwrap_or(self.tree.root_node());
        let mut best: Option<(usize, Node<'a>)> = None;
        let mut consider = |pos: usize, n: Node<'a>| {
            if best.is_none_or(|(b, _)| pos > b) {
                best = Some((pos, n));
            }
        };
        let is_var =
            |n: Option<Node>| n.is_some_and(|l| l.kind() == "variable_name" && self.text(l) == var);
        let mut stack = vec![scope];
        while let Some(n) = stack.pop() {
            if n.start_byte() >= usage.start_byte() {
                continue;
            }
            match n.kind() {
                "assignment_expression"
                    if n.end_byte() <= usage.start_byte()
                        && is_var(n.child_by_field_name("left")) =>
                {
                    consider(n.start_byte(), n)
                }
                "foreach_statement" if is_var(foreach_value(n)) => consider(n.start_byte(), n),
                "catch_clause" if is_var(n.child_by_field_name("name")) => {
                    consider(n.start_byte(), n)
                }
                "comment" if n.end_byte() <= usage.start_byte() => {
                    let raw = self.text(n);
                    if is_doc_comment(raw)
                        && parse_docblock(raw)
                            .tags
                            .iter()
                            .any(|t| matches!(&t.tag, Tag::Var { var: Some(v), .. } if format!("${v}") == var))
                    {
                        consider(n.start_byte(), n);
                    }
                }
                _ => {}
            }
            let mut c = n.walk();
            for child in n.named_children(&mut c) {
                if n.id() == scope.id() || !is_function_like(child.kind()) {
                    stack.push(child);
                }
            }
        }
        let owner = self.enclosing_class(usage, ctx);
        if let Some((_, n)) = best {
            return match n.kind() {
                "assignment_expression" => self.inline_var_doc(n, var, ctx).unwrap_or_else(|| {
                    n.child_by_field_name("right")
                        .map(|r| self.ty_of(r, depth))
                        .unwrap_or_default()
                }),
                "foreach_statement" => {
                    let documented =
                        foreach_value(n).and_then(|_| self.inline_var_doc(n, var, ctx));
                    documented.unwrap_or_else(|| {
                        let iterated = n
                            .named_child(0)
                            .map(|e| self.ty_of(e, depth))
                            .unwrap_or_default();
                        Ty {
                            class: iterated.elem,
                            elem: None,
                        }
                    })
                }
                "catch_clause" => n
                    .child_by_field_name("type")
                    .and_then(|t| t.named_child(0))
                    .and_then(|t| self.class_ref(t, ctx))
                    .map(Ty::class)
                    .unwrap_or_default(),
                _ => {
                    let doc = parse_docblock(self.text(n));
                    let ty = doc
                        .var_type(Some(var.trim_start_matches('$')))
                        .unwrap_or("");
                    resolve_doc_type(ty, ctx, owner.as_deref())
                }
            };
        }
        let Some(params) = scope.child_by_field_name("parameters") else {
            return Ty::default();
        };
        let mut c = params.walk();
        let Some(param) = params.named_children(&mut c).find(|p| {
            p.child_by_field_name("name")
                .is_some_and(|n| self.text(n) == var)
        }) else {
            return Ty::default();
        };
        let native = param
            .child_by_field_name("type")
            .map(|t| resolve_type(t, self.src, ctx, owner.as_deref()))
            .unwrap_or_default();
        let documented = doc_of(scope, self.src)
            .and_then(|d| {
                d.param_type(var.trim_start_matches('$'))
                    .map(|t| resolve_doc_type(t, ctx, owner.as_deref()))
            })
            .unwrap_or_default();
        let ty = native.or(documented);
        match owner {
            Some(o) => ty.bind_static(&o),
            None => ty,
        }
    }
}

/// The value variable of `foreach ($xs as $v)` / `foreach ($xs as $k => $v)`.
fn foreach_value(foreach: Node) -> Option<Node> {
    let value = foreach.named_child(1)?;
    let value = if value.kind() == "pair" {
        value.named_child(u32::try_from(value.named_child_count().checked_sub(1)?).ok()?)?
    } else {
        value
    };
    if value.kind() == "by_ref" {
        value.named_child(0)
    } else {
        Some(value)
    }
}

/// Node kinds that delimit a variable scope: functions, methods, closures, and arrow functions.
pub(crate) fn is_function_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition" | "method_declaration" | "anonymous_function" | "arrow_function"
    )
}

/// Innermost function-like ancestor of `node`, i.e. the scope its variables live in; `None` at file level.
pub(crate) fn enclosing_function(node: Node) -> Option<Node> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if is_function_like(n.kind()) {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

/// Go to declaration: resolve the identifier under the caret.
///
/// Parses `src` from scratch, so it works on unsaved buffers. Returns an empty
/// vec when the caret is not on a resolvable name. Used by the server for
/// definition, hover, and as the starting point of find usages.
pub fn definitions_at(index: &Index, src: &str, byte: usize) -> Vec<Definition> {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    analyzer
        .target_at(byte)
        .map(|(t, _)| resolve(index, &t))
        .unwrap_or_default()
}
