//! Reference resolution and type inference over one parsed file.
//!
//! Classifies the identifier under the caret by its syntactic position and
//! resolves it to declarations (possibly several) through the index.

use super::*;
use crate::index::Index;
use crate::laravel;
use crate::symbol::{
    Conditional, STATIC_TYPE, Symbol, Tag, Ty, is_doc_comment, parse_docblock, resolve_doc_type,
    template_name,
};
use crate::symbol::{MemberTag, doc_of, resolve_type};
use crate::syntax::{
    NameContext, UseKind, for_each_statement, is_class_like, parse, text, use_clauses,
};
use std::collections::{HashMap, HashSet, VecDeque};
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
        Target::Member { class, tag, name } => {
            let hits = index.find_member(class, *tag, name);
            // `User::where(...)`: Eloquent forwards to the query builder.
            match laravel::forward(index, &Ty::class(class.clone()), *tag) {
                Some(next) if hits.is_empty() => next
                    .class
                    .map(|c| index.find_member(&c, *tag, name))
                    .unwrap_or_default(),
                _ => hits,
            }
        }
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
    /// Name contexts of the top-level statements, built on first use by
    /// [`Analyzer::ctx`] (inspections ask for one per identifier).
    contexts: std::cell::OnceCell<StatementContexts>,
    /// Variable binding sites per scope (keyed by the scope node's id),
    /// built on first use by [`Analyzer::variable_type`].
    bindings: std::cell::RefCell<HashMap<usize, std::rc::Rc<Vec<Binding>>>>,
}

/// A place that gives a variable its type: an assignment, a `foreach`
/// value, a `catch` variable, or a `@var` doc comment naming it. Plain
/// byte ranges, so the cache does not tie the analyzer to node lifetimes.
#[derive(Clone)]
struct Binding {
    /// Byte range of the binding node.
    start: usize,
    /// End byte of the binding node.
    end: usize,
    /// The variable it binds, with `$`.
    var: String,
}

/// The name context of every top-level statement, in document order:
/// `(start, end, index into contexts)`, with each distinct context once.
#[derive(Default)]
struct StatementContexts {
    /// Statement ranges and their context.
    statements: Vec<(usize, usize, usize)>,
    /// Distinct contexts.
    contexts: Vec<NameContext>,
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
        Self {
            index,
            src,
            tree,
            contexts: std::cell::OnceCell::new(),
            bindings: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// Source text covered by `node`.
    pub(crate) fn text(&self, node: Node) -> &'a str {
        text(node, self.src)
    }

    /// Namespace and `use` imports in effect at `byte`, needed to qualify
    /// names written there. Same answer as [`crate::syntax::context_at`], from contexts
    /// computed once per analyzer.
    pub(crate) fn ctx(&self, byte: usize) -> NameContext {
        let cache = self.contexts.get_or_init(|| {
            let mut out = StatementContexts::default();
            for_each_statement(self.tree.root_node(), self.src, |stmt, ctx| {
                let same = out.contexts.last().is_some_and(|last| {
                    last.namespace == ctx.namespace
                        && last.classes == ctx.classes
                        && last.functions == ctx.functions
                });
                if !same {
                    out.contexts.push(ctx.clone());
                }
                let id = out.contexts.len() - 1;
                out.statements
                    .push((stmt.start_byte(), stmt.end_byte(), id));
            });
            out
        });
        let before = cache.statements.partition_point(|s| s.0 <= byte);
        // The first statement containing `byte`, else the last one before it.
        let containing = cache.statements[before.saturating_sub(2)..before]
            .iter()
            .find(|s| byte <= s.1);
        containing
            .or_else(|| before.checked_sub(1).map(|i| &cache.statements[i]))
            .map(|s| cache.contexts[s.2].clone())
            .unwrap_or_default()
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
            | "type_list"
            | "catch_clause"
            | "attribute"
            | "use_declaration"
            | "scoped_call_expression"
            | "scoped_property_access_expression"
            | "optional_type"
            | "union_type"
            | "intersection_type" => self.class_ref(node, &ctx).map(Target::Class),
            // Only `$x instanceof Foo` names a class; other operands are
            // constants (`__DIR__ . '/x'`, `PHP_EOL`).
            "binary_expression"
                if same(parent.child_by_field_name("right"), node)
                    && parent
                        .child_by_field_name("operator")
                        .is_some_and(|op| self.text(op).eq_ignore_ascii_case("instanceof")) =>
            {
                self.class_ref(node, &ctx).map(Target::Class)
            }
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
        let child = |field: &str| expr.child_by_field_name(field);
        match expr.kind() {
            "variable_name" => {
                let ctx = self.ctx(expr.start_byte());
                match self.text(expr) {
                    "$this" => Ty {
                        class: self.enclosing_class(expr, &ctx),
                        ..Ty::default()
                    },
                    var => self.variable_type(expr, var, &ctx, d),
                }
            }
            "object_creation_expression" => {
                let mut c = expr.walk();
                let class = expr.named_children(&mut c).find(|n| is_name(n.kind()));
                Ty {
                    class: class.and_then(|n| self.class_ref(n, &self.ctx(n.start_byte()))),
                    ..Ty::default()
                }
            }
            "member_call_expression" | "nullsafe_member_call_expression" => {
                let receiver = child("object")
                    .map(|o| self.ty_of(o, d))
                    .unwrap_or_default();
                self.member_ty(
                    receiver,
                    MemberTag::Method,
                    child("name"),
                    child("arguments"),
                )
            }
            "member_access_expression" | "nullsafe_member_access_expression" => {
                let receiver = child("object")
                    .map(|o| self.ty_of(o, d))
                    .unwrap_or_default();
                self.member_ty(receiver, MemberTag::Property, child("name"), None)
            }
            "scoped_call_expression" => {
                let receiver = Ty {
                    class: child("scope").and_then(|s| self.class_of_scope(s, d)),
                    ..Ty::default()
                };
                self.member_ty(
                    receiver,
                    MemberTag::Method,
                    child("name"),
                    child("arguments"),
                )
            }
            "scoped_property_access_expression" => {
                let receiver = Ty {
                    class: child("scope").and_then(|s| self.class_of_scope(s, d)),
                    ..Ty::default()
                };
                self.member_ty(
                    receiver,
                    MemberTag::Property,
                    child("name").and_then(|n| n.named_child(0)),
                    None,
                )
            }
            "class_constant_access_expression" => {
                let receiver = Ty {
                    class: expr.named_child(0).and_then(|s| self.class_of_scope(s, d)),
                    ..Ty::default()
                };
                self.member_ty(receiver, MemberTag::Constant, expr.named_child(1), None)
            }
            "function_call_expression" => {
                let Some(f) = child("function") else {
                    return Ty::default();
                };
                // `app('cache')`: a core container alias names its class.
                if let Some(class) = child("arguments")
                    .and_then(|args| args.named_child(0))
                    .and_then(|arg| arg.named_child(0))
                    .filter(|v| v.kind() == "string")
                    .and_then(|v| {
                        let name = self.text(v).trim_matches(['\'', '"']);
                        laravel::container_binding(self.text(f), name)
                    })
                {
                    return Ty::class(class);
                }
                let ctx = self.ctx(f.start_byte());
                let found = self.index.functions(&ctx.resolve_function(self.text(f)));
                let Some((_, s)) = found.first() else {
                    return Ty::default();
                };
                let mut bindings = HashMap::new();
                let arguments = child("arguments");
                self.bind_class_strings(s, arguments, &mut bindings);
                let ty =
                    s.ty.clone()
                        .select(&|c| self.decide(c, arguments))
                        .substitute(&bindings);
                laravel::refine(self.index, ty)
            }
            "subscript_expression" => {
                let elem = expr.named_child(0).and_then(|a| self.ty_of(a, d).elem);
                Ty {
                    class: elem,
                    ..Ty::default()
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

    /// Type of a member accessed on `receiver`: its declared type with the
    /// receiver's generic arguments and any `X::class` arguments bound to
    /// the member's templates, and `static` bound to the receiver.
    fn member_ty(
        &self,
        receiver: Ty,
        tag: MemberTag,
        name: Option<Node>,
        arguments: Option<Node>,
    ) -> Ty {
        self.member_ty_via(receiver, tag, name, arguments, true)
    }

    /// [`Self::member_ty`]; when the member is missing and `forward` is set,
    /// retries once on the receiver Eloquent forwards the call to.
    fn member_ty_via(
        &self,
        receiver: Ty,
        tag: MemberTag,
        name: Option<Node>,
        arguments: Option<Node>,
        forward: bool,
    ) -> Ty {
        let (Some(class), Some(name_node)) = (receiver.class.clone(), name) else {
            return Ty::default();
        };
        let found = self.index.find_member(&class, tag, self.text(name_node));
        let Some((_, s)) = found.first() else {
            return match laravel::forward(self.index, &receiver, tag) {
                Some(next) if forward => self.member_ty_via(next, tag, name, arguments, false),
                _ => Ty::default(),
            };
        };
        let mut bindings = generic_bindings(self.index, &class, &receiver.args, &s.fqn);
        self.bind_class_strings(s, arguments, &mut bindings);
        let ty =
            s.ty.clone()
                .select(&|c| self.decide(c, arguments))
                .substitute(&bindings);
        let ty = laravel::refine(self.index, ty);
        let ty = match laravel::untyped_return(self.index, &class, self.text(name_node)) {
            Some(known) if ty.is_empty() && tag == MemberTag::Method => known,
            _ => ty,
        };
        // `static` keeps the receiver's generic arguments (`filter(): static`).
        if ty.class.as_deref() == Some(STATIC_TYPE) {
            return Ty {
                class: Some(class),
                elem: receiver.elem,
                args: receiver.args,
                conditional: None,
            };
        }
        ty.bind_static(&class)
    }

    /// Whether the argument a call passes for a conditional type's parameter
    /// matches the condition's test, judged from literals only: a missing
    /// argument counts as `null`, `Foo::class` as a class string, and
    /// strings, numbers, booleans and array literals as their types.
    /// `None` for anything else (variables, calls).
    fn decide(&self, cond: &Conditional, arguments: Option<Node>) -> Option<bool> {
        let index = cond.index?;
        let arg = arguments.and_then(|a| {
            let mut c = a.walk();
            let args: Vec<Node> = a.named_children(&mut c).collect();
            args.get(index).and_then(|n| {
                if n.kind() == "argument" {
                    n.named_children(&mut n.walk()).last()
                } else {
                    Some(*n)
                }
            })
        });
        let test = cond.test.to_ascii_lowercase();
        let parts: Vec<&str> = test
            .trim_matches(|c| c == '(' || c == ')')
            .split('|')
            .map(|p| p.trim().trim_matches(|c| c == '(' || c == ')'))
            .collect();
        let any = |f: &dyn Fn(&str) -> bool| Some(parts.iter().any(|p| f(p)));
        let Some(arg) = arg else {
            return any(&|p| p == "null");
        };
        match arg.kind() {
            "null" => any(&|p| p == "null"),
            "string" | "encapsed_string" | "heredoc" | "nowdoc" => {
                any(&|p| p.ends_with("string") && !p.starts_with("class-string"))
            }
            "integer" => any(&|p| p.ends_with("int") || p == "integer" || p == "numeric"),
            "float" => any(&|p| p == "float" || p == "numeric"),
            "boolean" => any(&|p| p == "bool" || p == "true" || p == "false"),
            "array_creation_expression" => any(&|p| {
                p.starts_with("array") || p.starts_with("list") || p.starts_with("iterable")
            }),
            "class_constant_access_expression"
                if arg
                    .named_child(1)
                    .is_some_and(|n| self.text(n).eq_ignore_ascii_case("class")) =>
            {
                any(&|p| p.starts_with("class-string") || p == "string")
            }
            _ => None,
        }
    }

    /// Binds templates of `callee` declared as `class-string<T>` parameters
    /// to the class of a matching `Foo::class` argument.
    fn bind_class_strings(
        &self,
        callee: &Symbol,
        arguments: Option<Node>,
        bindings: &mut HashMap<String, String>,
    ) {
        let Some(arguments) = arguments else { return };
        if callee.class_string_params.is_empty() {
            return;
        }
        let mut c = arguments.walk();
        let args: Vec<Node> = arguments.named_children(&mut c).collect();
        for (index, template) in &callee.class_string_params {
            let Some(value) = args.get(*index).and_then(|a| a.named_child(0)) else {
                continue;
            };
            let is_class_constant = value.kind() == "class_constant_access_expression"
                && value
                    .named_child(1)
                    .is_some_and(|n| self.text(n).eq_ignore_ascii_case("class"));
            if let Some(class) = value
                .named_child(0)
                .filter(|_| is_class_constant)
                .and_then(|s| self.class_of_scope(s, 0))
            {
                bindings.insert(template.clone(), class);
            }
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

    /// Every binding site in `scope` (not inside nested functions), computed
    /// once per scope: variable type lookups only filter it by name and
    /// position.
    fn scope_bindings(&self, scope: Node<'a>) -> std::rc::Rc<Vec<Binding>> {
        if let Some(found) = self.bindings.borrow().get(&scope.id()) {
            return found.clone();
        }
        let mut out = Vec::new();
        let var_of = |n: Option<Node>| {
            n.filter(|l| l.kind() == "variable_name")
                .map(|l| self.text(l).to_string())
        };
        let mut stack = vec![scope];
        while let Some(n) = stack.pop() {
            let mut bind = |var: String| {
                out.push(Binding {
                    start: n.start_byte(),
                    end: n.end_byte(),
                    var,
                });
            };
            match n.kind() {
                "assignment_expression" => var_of(n.child_by_field_name("left"))
                    .map(&mut bind)
                    .unwrap_or(()),
                "foreach_statement" => var_of(foreach_value(n)).map(&mut bind).unwrap_or(()),
                "catch_clause" => var_of(n.child_by_field_name("name"))
                    .map(&mut bind)
                    .unwrap_or(()),
                "comment" => {
                    let raw = self.text(n);
                    if is_doc_comment(raw) {
                        for tag in parse_docblock(raw).tags {
                            if let Tag::Var { var: Some(v), .. } = &tag.tag {
                                bind(format!("${v}"));
                            }
                        }
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
        let out = std::rc::Rc::new(out);
        self.bindings.borrow_mut().insert(scope.id(), out.clone());
        out
    }

    /// The node of `kind` spanning exactly `start..end`.
    fn node_spanning(&self, start: usize, end: usize, kinds: &[&str]) -> Option<Node<'a>> {
        let mut n = self
            .tree
            .root_node()
            .descendant_for_byte_range(start, end)?;
        loop {
            if n.start_byte() == start && n.end_byte() == end && kinds.contains(&n.kind()) {
                return Some(n);
            }
            n = n.parent()?;
        }
    }

    /// Type of `$var` at `usage`. The source closest before the usage wins:
    /// an assignment (or its `@var`), a `foreach` binding, a `catch` variable,
    /// or a standalone `/** @var Type $var */`. Parameters (native type, then
    /// `@param`) are the fallback.
    fn variable_type(&self, usage: Node<'a>, var: &str, ctx: &NameContext, depth: u8) -> Ty {
        let scope = enclosing_function(usage).unwrap_or(self.tree.root_node());
        let bindings = self.scope_bindings(scope);
        let before = usage.start_byte();
        let kinds = [
            "assignment_expression",
            "foreach_statement",
            "catch_clause",
            "comment",
        ];
        let best = bindings
            .iter()
            .filter(|b| b.var == var && b.start < before)
            .filter_map(|b| Some((b.start, self.node_spanning(b.start, b.end, &kinds)?)))
            .filter(|(_, n)| match n.kind() {
                "assignment_expression" | "comment" => n.end_byte() <= before,
                _ => true,
            })
            .max_by_key(|(pos, _)| *pos);
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
                            ..Ty::default()
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

/// Template bindings for members declared on `owner` when accessed through
/// `class` with generic arguments `args`, following `@extends` /
/// `@implements` / `@use` arguments up the hierarchy.
fn generic_bindings(
    index: &Index,
    class: &str,
    args: &[String],
    owner: &str,
) -> HashMap<String, String> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([(class.to_string(), args.to_vec())]);
    while let Some((cls, cls_args)) = queue.pop_front() {
        if !seen.insert(cls.to_ascii_lowercase()) {
            continue;
        }
        // A class may be declared more than once (vendor stubs): take the
        // templates from the first declaration that has them and the
        // supertypes of all.
        let decls = index.classes(&cls);
        if decls.is_empty() {
            continue;
        }
        let templates = decls
            .iter()
            .map(|(_, d)| &d.templates)
            .find(|t| !t.is_empty())
            .cloned()
            .unwrap_or_default();
        let own: HashMap<String, String> = templates
            .into_iter()
            .zip(cls_args.iter().cloned())
            .collect();
        if cls.eq_ignore_ascii_case(owner) {
            return own;
        }
        let super_args: Vec<&(String, Vec<String>)> = decls
            .iter()
            .flat_map(|(_, d)| d.super_args.iter())
            .collect();
        let supers: Vec<&String> = decls.iter().flat_map(|(_, d)| d.supers.iter()).collect();
        for sup in supers {
            // Arguments given to this supertype, expressed in our own templates.
            let given = super_args
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(sup))
                .map(|(_, a)| {
                    a.iter()
                        .map(|x| match template_name(x) {
                            Some(t) => own.get(t).cloned().unwrap_or_default(),
                            None => x.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            queue.push_back((sup.clone(), given));
        }
    }
    HashMap::new()
}
