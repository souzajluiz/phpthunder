//! Reference resolution, find usages, and completion over one parsed file.
//!
//! - [`Analyzer::target_at`] + [`resolve`]: classify the identifier under the
//!   caret by syntax position and resolve it to declarations (possibly several).
//! - [`find_references`]: the word index narrows candidate files, then each
//!   occurrence is resolved and kept only if it resolves to the searched
//!   declaration.
//! - [`complete`]: a placeholder identifier is inserted at the caret so the
//!   incomplete code parses, then the placeholder's position decides what to
//!   offer.

use crate::index::Index;
use crate::stubs::{resolve_type, Kind, MemberTag, Symbol, STATIC_TYPE};
use crate::syntax::{
    context_at, import_insertion_point, is_class_like, parse, text, use_clauses, NameContext,
    UseKind,
};
use crate::text::{LineIndex, Span};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Tree};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Class(String),
    /// Candidate FQNs in lookup order.
    Function(Vec<String>),
    Member { class: String, tag: MemberTag, name: String },
}

/// A resolved declaration, owned so it can outlive the index lock.
#[derive(Clone, Debug)]
pub struct Definition {
    pub path: PathBuf,
    pub symbol: Symbol,
}

impl Definition {
    fn identity(&self) -> (&Path, usize) {
        (&self.path, self.symbol.name_start)
    }
}

pub fn resolve(index: &Index, target: &Target) -> Vec<Definition> {
    let hits = match target {
        Target::Class(fqn) => index.classes(fqn),
        Target::Function(candidates) => index.functions(candidates),
        Target::Member { class, tag, name } => index.find_member(class, *tag, name),
    };
    hits.into_iter()
        .map(|(p, s)| Definition { path: p.to_path_buf(), symbol: s.clone() })
        .collect()
}

const MAX_INFERENCE_DEPTH: u8 = 8;

pub struct Analyzer<'a> {
    pub index: &'a Index,
    pub src: &'a str,
    pub tree: &'a Tree,
}

fn is_name(kind: &str) -> bool {
    matches!(kind, "name" | "qualified_name" | "relative_name")
}

fn same(a: Option<Node>, b: Node) -> bool {
    a.is_some_and(|a| a.id() == b.id())
}

impl<'a> Analyzer<'a> {
    pub fn new(index: &'a Index, src: &'a str, tree: &'a Tree) -> Self {
        Self { index, src, tree }
    }

    fn text(&self, node: Node) -> &'a str {
        text(node, self.src)
    }

    fn ctx(&self, byte: usize) -> NameContext {
        context_at(self.tree.root_node(), self.src, byte)
    }

    /// The identifier node under the caret (the caret may sit just after it).
    pub fn name_node_at(&self, byte: usize) -> Option<Node<'a>> {
        let root = self.tree.root_node();
        [byte, byte.saturating_sub(1)].into_iter().find_map(|b| {
            let node = root.named_descendant_for_byte_range(b, b)?;
            let node = if node.kind() == "variable_name" { node.named_child(0)? } else { node };
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
        if let Some(p) = node.parent().filter(|p| matches!(p.kind(), "qualified_name" | "relative_name")) {
            node = p;
        }
        let parent = node.parent()?;
        let raw = self.text(node);
        let ctx = self.ctx(node.start_byte());
        let member = |class: Option<String>, tag| {
            Some(Target::Member { class: class?, tag, name: raw.to_string() })
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
                let decl = parent.parent().filter(|d| d.kind() == "namespace_use_declaration")
                    .or_else(|| parent.parent()?.parent())?;
                let clause = use_clauses(decl, self.src).into_iter().find(|c| {
                    c.name_node.id() == node.id() || same(c.alias_node, node)
                })?;
                match clause.kind {
                    UseKind::Class => Some(Target::Class(clause.fqn)),
                    UseKind::Function => Some(Target::Function(vec![clause.fqn])),
                    UseKind::Const => None,
                }
            }
            "named_type" | "base_clause" | "class_interface_clause" | "object_creation_expression"
            | "binary_expression" | "type_list" | "catch_clause" | "attribute" | "use_declaration"
            | "scoped_call_expression" | "scoped_property_access_expression" | "optional_type"
            | "union_type" | "intersection_type" => self.class_ref(node, &ctx).map(Target::Class),
            _ => None,
        }
    }

    /// Resolves a class name as written, handling `self`, `static`, and `parent`.
    fn class_ref(&self, node: Node, ctx: &NameContext) -> Option<String> {
        let raw = self.text(node);
        match raw.to_ascii_lowercase().as_str() {
            "self" | "static" => self.enclosing_class(node, ctx),
            "parent" => {
                let cls = self.enclosing_class(node, ctx)?;
                self.index.classes(&cls).into_iter().find_map(|(_, s)| s.extends.clone())
            }
            _ => Some(ctx.resolve_class(raw)),
        }
    }

    fn enclosing_class(&self, node: Node, ctx: &NameContext) -> Option<String> {
        let mut cur = node.parent();
        while let Some(n) = cur {
            if is_class_like(n.kind()) {
                return Some(ctx.qualify(self.text(n.child_by_field_name("name")?)));
            }
            cur = n.parent();
        }
        None
    }

    fn class_of_scope(&self, scope: Node, depth: u8) -> Option<String> {
        if is_name(scope.kind()) {
            self.class_ref(scope, &self.ctx(scope.start_byte()))
        } else {
            self.type_of(scope, depth)
        }
    }

    /// Lightweight type inference: the class an expression evaluates to.
    pub fn type_of(&self, expr: Node, depth: u8) -> Option<String> {
        if depth > MAX_INFERENCE_DEPTH {
            return None;
        }
        let d = depth + 1;
        let member_type = |class: Option<String>, tag, name: Option<Node>| -> Option<String> {
            let class = class?;
            let name = self.text(name?);
            let found = self.index.find_member(&class, tag, name);
            let ty = found.first()?.1.ty.clone()?;
            Some(if ty == STATIC_TYPE { class } else { ty })
        };
        match expr.kind() {
            "variable_name" => {
                let ctx = self.ctx(expr.start_byte());
                match self.text(expr) {
                    "$this" => self.enclosing_class(expr, &ctx),
                    var => self.variable_type(expr, var, &ctx, d),
                }
            }
            "object_creation_expression" => {
                let mut c = expr.walk();
                let class = expr.named_children(&mut c).find(|n| is_name(n.kind()))?;
                self.class_ref(class, &self.ctx(class.start_byte()))
            }
            "member_call_expression" | "nullsafe_member_call_expression" => member_type(
                self.type_of(expr.child_by_field_name("object")?, d),
                MemberTag::Method,
                expr.child_by_field_name("name"),
            ),
            "member_access_expression" | "nullsafe_member_access_expression" => member_type(
                self.type_of(expr.child_by_field_name("object")?, d),
                MemberTag::Property,
                expr.child_by_field_name("name"),
            ),
            "scoped_call_expression" => member_type(
                self.class_of_scope(expr.child_by_field_name("scope")?, d),
                MemberTag::Method,
                expr.child_by_field_name("name"),
            ),
            "scoped_property_access_expression" => member_type(
                self.class_of_scope(expr.child_by_field_name("scope")?, d),
                MemberTag::Property,
                expr.child_by_field_name("name")?.named_child(0),
            ),
            "class_constant_access_expression" => member_type(
                self.class_of_scope(expr.named_child(0)?, d),
                MemberTag::Constant,
                expr.named_child(1),
            ),
            "function_call_expression" => {
                let f = expr.child_by_field_name("function")?;
                let ctx = self.ctx(f.start_byte());
                let found = self.index.functions(&ctx.resolve_function(self.text(f)));
                found.first()?.1.ty.clone()
            }
            "parenthesized_expression" => self.type_of(expr.named_child(0)?, d),
            "assignment_expression" => self.type_of(expr.child_by_field_name("right")?, d),
            _ => None,
        }
    }

    /// Type of `$var` at `usage`: last assignment before the usage in the same
    /// function, otherwise the parameter's declared type.
    fn variable_type(&self, usage: Node, var: &str, ctx: &NameContext, depth: u8) -> Option<String> {
        let scope = enclosing_function(usage).unwrap_or(self.tree.root_node());
        let mut best: Option<Node> = None;
        let mut stack = vec![scope];
        while let Some(n) = stack.pop() {
            if n.start_byte() >= usage.start_byte() {
                continue;
            }
            if n.kind() == "assignment_expression" && n.end_byte() <= usage.start_byte() {
                let left = n.child_by_field_name("left");
                if left.is_some_and(|l| l.kind() == "variable_name" && self.text(l) == var)
                    && best.is_none_or(|b| n.start_byte() > b.start_byte())
                {
                    best = Some(n);
                }
            }
            let mut c = n.walk();
            for child in n.named_children(&mut c) {
                if n.id() == scope.id() || !is_function_like(child.kind()) {
                    stack.push(child);
                }
            }
        }
        if let Some(assign) = best {
            return self.type_of(assign.child_by_field_name("right")?, depth);
        }
        let params = scope.child_by_field_name("parameters")?;
        let mut c = params.walk();
        let param = params.named_children(&mut c).find(|p| {
            p.child_by_field_name("name").is_some_and(|n| self.text(n) == var)
        })?;
        let owner = self.enclosing_class(param, ctx);
        let ty = resolve_type(param.child_by_field_name("type")?, self.src, ctx, owner.as_deref())?;
        if ty == STATIC_TYPE { owner } else { Some(ty) }
    }
}

fn is_function_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition" | "method_declaration" | "anonymous_function" | "arrow_function"
    )
}

fn enclosing_function(node: Node) -> Option<Node> {
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
pub fn definitions_at(index: &Index, src: &str, byte: usize) -> Vec<Definition> {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    analyzer
        .target_at(byte)
        .map(|(t, _)| resolve(index, &t))
        .unwrap_or_default()
}

pub struct Usage {
    pub path: PathBuf,
    pub span: Span,
    pub is_declaration: bool,
}

/// Find usages of the declarations `defs`.
///
/// `load` returns current file content (open editor buffers win over disk).
pub fn find_references(
    index: &Index,
    defs: &[Definition],
    load: impl Fn(&Path) -> Option<String>,
) -> Vec<Usage> {
    let Some(first) = defs.first() else { return Vec::new() };
    let word = first.symbol.name.as_str();
    let wanted: HashSet<(&Path, usize)> = defs.iter().map(Definition::identity).collect();
    let mut out = Vec::new();
    for path in index.files_with_word(word) {
        let Some(src) = load(&path) else { continue };
        let tree = parse(&src);
        let analyzer = Analyzer::new(index, &src, &tree);
        let lines = LineIndex::new(&src);
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            let mut c = node.walk();
            stack.extend(node.named_children(&mut c));
            if node.kind() != "name" || !text(node, &src).eq_ignore_ascii_case(word) {
                continue;
            }
            let Some(target) = analyzer.target_of(node) else { continue };
            let hit = resolve(index, &target)
                .iter()
                .any(|d| wanted.contains(&d.identity()));
            if hit {
                out.push(Usage {
                    path: path.clone(),
                    span: lines.span(&src, node.start_byte(), node.end_byte()),
                    is_declaration: wanted.contains(&(path.as_path(), node.start_byte())),
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.path, a.span.start.line, a.span.start.col).cmp(&(&b.path, b.span.start.line, b.span.start.col)));
    out
}

pub const CARET_PLACEHOLDER: &str = "PhpThunderCaret";

#[derive(Clone, Debug)]
pub struct Candidate {
    pub label: String,
    pub kind: Kind,
    pub detail: String,
    pub insert_text: String,
    /// `use` statement to add, as (byte offset, text).
    pub import: Option<(usize, String)>,
}

/// Completion at `byte`.
pub fn complete(index: &Index, src: &str, byte: usize) -> Vec<Candidate> {
    // Variables need no placeholder: half-typed `$x` often confuses error recovery.
    let word = ident_start(src, byte);
    if word > 0 && src.as_bytes()[word - 1] == b'$' {
        return complete_variables(src, word - 1, byte);
    }
    let patched = format!("{}{}{}", &src[..byte], CARET_PLACEHOLDER, &src[byte..]);
    let tree = parse(&patched);
    let analyzer = Analyzer::new(index, &patched, &tree);
    let root = tree.root_node();
    let Some(mut node) = root.named_descendant_for_byte_range(byte, byte + 1) else {
        return Vec::new();
    };
    if node.kind() != "name" {
        return Vec::new();
    }
    let prefix = &patched[node.start_byte()..byte];
    let Some(mut parent) = node.parent() else { return Vec::new() };
    if matches!(parent.kind(), "namespace_name" | "qualified_name") {
        // Namespaced names are completed as a whole; keep it simple.
        return Vec::new();
    }
    if parent.kind() == "variable_name" {
        node = parent;
        parent = node.parent().unwrap_or(parent);
    }

    let members = |class: Option<String>, statics: bool| -> Vec<Candidate> {
        let Some(class) = class else { return Vec::new() };
        index
            .members(&class)
            .into_iter()
            .filter(|(_, s)| s.name.to_lowercase().starts_with(&prefix.to_lowercase()))
            .filter(|(_, s)| if statics { s.is_static } else { s.kind == Kind::Method || !s.is_static })
            .map(|(_, s)| {
                let name = if statics && s.kind == Kind::Property { format!("${}", s.name) } else { s.name.clone() };
                let insert = if s.kind == Kind::Method { format!("{name}(") } else { name.clone() };
                Candidate { label: name, kind: s.kind, detail: s.signature.clone(), insert_text: insert, import: None }
            })
            .collect()
    };

    match parent.kind() {
        "member_call_expression" | "member_access_expression" | "nullsafe_member_call_expression"
        | "nullsafe_member_access_expression" => {
            let object = parent.child_by_field_name("object");
            members(object.and_then(|o| analyzer.type_of(o, 0)), false)
        }
        "scoped_call_expression" | "scoped_property_access_expression" => {
            let scope = parent.child_by_field_name("scope");
            members(scope.and_then(|s| analyzer.class_of_scope(s, 0)), true)
        }
        "class_constant_access_expression" if parent.named_child(0).is_some_and(|s| s.id() != node.id()) => {
            let scope = parent.named_child(0).unwrap();
            let mut out = members(analyzer.class_of_scope(scope, 0), true);
            if "class".starts_with(&prefix.to_lowercase()) {
                out.push(Candidate {
                    label: "class".into(),
                    kind: Kind::ClassConstant,
                    detail: "class name".into(),
                    insert_text: "class".into(),
                    import: None,
                });
            }
            out
        }
        k if is_class_like(k) || matches!(k, "function_definition" | "method_declaration" | "const_element" | "namespace_definition") => Vec::new(),
        _ => complete_names(&analyzer, node, prefix, src, byte, parent.kind()),
    }
}

/// Start of the identifier that ends at `byte` (without a leading `$`).
pub fn ident_start(src: &str, byte: usize) -> usize {
    let bytes = src.as_bytes();
    let mut i = byte;
    while i > 0 && (bytes[i - 1] == b'_' || bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] >= 0x80) {
        i -= 1;
    }
    i
}

/// Variables visible in the function around `start` (the `$` being typed).
fn complete_variables(src: &str, start: usize, byte: usize) -> Vec<Candidate> {
    let prefix = &src[start..byte];
    let tree = parse(src);
    let here = tree.root_node().descendant_for_byte_range(start, start);
    let function = here.and_then(enclosing_function);
    let scope = function.unwrap_or(tree.root_node());
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            if n.id() == scope.id() || !is_function_like(child.kind()) {
                stack.push(child);
            }
        }
        if n.kind() == "variable_name" && n.start_byte() != start {
            names.push(text(n, src));
        }
    }
    if function.is_some_and(|f| f.kind() == "method_declaration") {
        names.push("$this");
    }
    names
        .into_iter()
        .filter(|name| name.starts_with(prefix) && *name != prefix && seen.insert(*name))
        .map(|name| Candidate {
            label: name.to_string(),
            kind: Kind::Property,
            detail: "variable".into(),
            insert_text: name.to_string(),
            import: None,
        })
        .collect()
}

fn complete_names(
    analyzer: &Analyzer,
    node: Node,
    prefix: &str,
    original: &str,
    byte: usize,
    parent_kind: &str,
) -> Vec<Candidate> {
    let ctx = analyzer.ctx(node.start_byte());
    // Function calls are possible only where an expression is.
    let type_position = matches!(
        parent_kind,
        "named_type" | "base_clause" | "class_interface_clause" | "object_creation_expression"
            | "catch_clause" | "type_list" | "attribute" | "use_declaration" | "optional_type"
            | "union_type"
    );
    let original_tree = parse(original);
    let import_at = import_insertion_point(original_tree.root_node(), byte);
    analyzer
        .index
        .top_level_with_prefix(prefix, 200)
        .into_iter()
        .filter(|(_, s)| s.kind.is_class_like() || !type_position)
        .map(|(_, s)| {
            let short = s.fqn.rsplit('\\').next().unwrap_or(&s.fqn).to_string();
            let namespace = s.fqn.rsplit_once('\\').map(|(ns, _)| ns).unwrap_or("");
            let foreign = !namespace.eq_ignore_ascii_case(&ctx.namespace);
            let mut import = None;
            let mut insert = short.clone();
            if s.kind.is_class_like() {
                if foreign && !ctx.imports_class(&s.fqn) {
                    if ctx.classes.contains_key(&short.to_ascii_lowercase()) {
                        insert = format!("\\{}", s.fqn); // alias clash: fully qualify
                    } else if !namespace.is_empty() || !ctx.namespace.is_empty() {
                        import = Some((import_at, format!("\nuse {};", s.fqn)));
                    }
                }
            } else if foreign && !namespace.is_empty() {
                insert = format!("\\{}", s.fqn);
            }
            if s.kind == Kind::Function {
                insert.push('(');
            }
            Candidate { label: short, kind: s.kind, detail: s.fqn.clone(), insert_text: insert, import }
        })
        .collect()
}
