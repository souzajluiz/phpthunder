//! Inspections that need the project index: class hierarchies (missing
//! implementations, `final`, override visibility, `static` and signature
//! compatibility, supertypes of the wrong kind), calls (argument counts,
//! `void` results, static calls to instance methods), member access
//! (private/protected members, readonly properties, class constants),
//! `catch` order, exhaustive enum `match`es, parent constructor calls and
//! PHPUnit data providers.
//!
//! Like the other index-backed inspections, these only report what the
//! index knows for sure: a class whose hierarchy is not fully indexed, or
//! has magic methods, is left alone.

use super::*;
use crate::analysis::{Analyzer, Target};
use crate::index::{Index, member_key};
use crate::refactor::{implementation_stub, missing_implementations};
use crate::symbol::{Kind, MemberTag, Symbol, Visibility};
use crate::syntax::{NameContext, is_class_like, parse, text};
use std::collections::HashSet;
use tree_sitter::Node;

/// Runs every index-aware rule over the tree rooted at `root`.
///
/// Called by [`diagnostics`] on each inspection pass.
pub(crate) fn semantic_checks(
    index: &Index,
    analyzer: &Analyzer,
    root: Node,
    src: &str,
    out: &mut Findings,
) {
    for node in descendants(root) {
        if node.is_error() {
            continue;
        }
        match node.kind() {
            "class_declaration" | "enum_declaration" | "interface_declaration" => {
                class_checks(index, analyzer, node, src, out);
            }
            "object_creation_expression" => {
                instantiation(index, analyzer, node, src, out);
                argument_count(index, analyzer, node, out);
            }
            "function_call_expression"
            | "member_call_expression"
            | "scoped_call_expression"
            | "nullsafe_member_call_expression" => {
                argument_count(index, analyzer, node, out);
                void_result(index, analyzer, node, out);
                if node.kind() == "scoped_call_expression" {
                    static_call(index, analyzer, node, src, out);
                }
            }
            "class_constant_access_expression" => class_constant(index, analyzer, node, src, out),
            "try_statement" => catch_order(index, analyzer, node, src, out),
            "match_expression" => enum_match(index, analyzer, node, src, out),
            "assignment_expression" => readonly_write(index, analyzer, node, src, out),
            "method_declaration" => data_provider(index, analyzer, node, src, out),
            _ => {}
        }
        if let Some(name) = member_name(node) {
            access(index, analyzer, node, name, src, out);
        }
    }
}

/// The member name node of a member access or call.
fn member_name(node: Node) -> Option<Node> {
    match node.kind() {
        "member_call_expression"
        | "nullsafe_member_call_expression"
        | "member_access_expression"
        | "nullsafe_member_access_expression"
        | "scoped_call_expression" => node.child_by_field_name("name"),
        "scoped_property_access_expression" => node
            .child_by_field_name("name")
            .and_then(|v| v.named_child(0).or(Some(v))),
        "class_constant_access_expression" => node.named_child(1),
        _ => None,
    }
}

/// Whether every class in `class`'s hierarchy is indexed (so a missing
/// member or method really is missing). Pseudo-supertypes such as a
/// model's `<table>` are not classes and do not count.
fn fully_indexed(index: &Index, class: &str) -> bool {
    index
        .hierarchy(class)
        .iter()
        .all(|c| c.starts_with('<') || !index.classes(c).is_empty())
}

/// Whether `class` (or a supertype) declares one of the magic `methods`.
fn has_magic(index: &Index, class: &str, methods: &[&str]) -> bool {
    methods
        .iter()
        .any(|m| !index.find_member(class, MemberTag::Method, m).is_empty())
}

/// Name context at `node`.
fn ctx_of(analyzer: &Analyzer, node: Node) -> NameContext {
    analyzer.ctx(node.start_byte())
}

/// The class-like symbol named `fqn`, if indexed once.
fn class_symbol<'i>(index: &'i Index, fqn: &str) -> Option<&'i Symbol> {
    let found = index.classes(fqn);
    (found.len() == 1).then(|| found[0].1)
}

/// Supertype names written in `clause` (`extends` / `implements`).
fn clause_names<'t>(clause: Node<'t>) -> Vec<Node<'t>> {
    let mut c = clause.walk();
    clause
        .named_children(&mut c)
        .filter(|n| matches!(n.kind(), "name" | "qualified_name" | "relative_name"))
        .collect()
}

/// Per-class checks: supertypes, missing implementations, overrides and
/// the parent constructor call.
fn class_checks(index: &Index, analyzer: &Analyzer, class: Node, src: &str, out: &mut Findings) {
    let Some(name) = class.child_by_field_name("name") else {
        return;
    };
    let ctx = ctx_of(analyzer, class);
    let fqn = ctx.qualify(text(name, src));
    let mut c = class.walk();
    for clause in class.named_children(&mut c) {
        let implements = clause.kind() == "class_interface_clause";
        if !implements && clause.kind() != "base_clause" {
            continue;
        }
        for target in clause_names(clause) {
            let resolved = ctx.resolve_class(text(target, src));
            let Some(sym) = class_symbol(index, &resolved) else {
                continue;
            };
            let wrong = match (class.kind(), implements) {
                ("interface_declaration", _) => sym.kind != Kind::Interface,
                (_, true) => sym.kind != Kind::Interface,
                (_, false) => sym.kind != Kind::Class,
            };
            if wrong {
                out.add(target, Rule::WrongSupertypeKind, text(target, src));
            } else if !implements && sym.is_final {
                out.add(target, Rule::ExtendsFinalClass, text(target, src));
            }
        }
    }
    if class.kind() == "interface_declaration" || !fully_indexed(index, &fqn) {
        return;
    }
    let Some(body) = class.child_by_field_name("body") else {
        return;
    };
    let mut bc = body.walk();
    let methods: Vec<Node> = body
        .named_children(&mut bc)
        .filter(|m| m.kind() == "method_declaration")
        .collect();
    let own: HashSet<String> = methods
        .iter()
        .filter_map(|m| m.child_by_field_name("name"))
        .map(|n| text(n, src).to_ascii_lowercase())
        .collect();
    let is_abstract = {
        let mut c = class.walk();
        class
            .children(&mut c)
            .any(|n| n.kind() == "abstract_modifier")
    };
    if !is_abstract {
        let missing: Vec<String> = missing_implementations(index, &fqn)
            .into_iter()
            .filter(|m| !own.contains(&m.name.to_ascii_lowercase()))
            .map(|m| format!("{}()", m.name))
            .collect();
        if !missing.is_empty() {
            out.add(name, Rule::MissingImplementation, missing.join(", "));
        }
    }
    for method in methods {
        overrides(index, &fqn, method, src, out);
    }
    parent_constructor(index, class, &fqn, src, out);
}

/// The method `name` declared by a strict supertype of `class`, nearest
/// first, skipping private ones (they are not inherited).
fn inherited<'i>(index: &'i Index, class: &str, name: &str) -> Option<&'i Symbol> {
    index.hierarchy(class).iter().skip(1).find_map(|cls| {
        index
            .lookup(&member_key(cls, MemberTag::Method, name))
            .into_iter()
            .map(|(_, s)| s)
            .find(|s| s.visibility != Visibility::Private && !s.is_virtual)
    })
}

/// How restrictive a visibility is, for comparing overrides.
fn rank(v: Visibility) -> u8 {
    match v {
        Visibility::Public => 0,
        Visibility::Protected => 1,
        Visibility::Private => 2,
    }
}

/// The written visibility of a member declaration node.
fn written_visibility(member: Node, src: &str) -> Visibility {
    let mut c = member.walk();
    member
        .children(&mut c)
        .find(|n| n.kind() == "visibility_modifier")
        .map_or(Visibility::Public, |v| match text(v, src) {
            "private" => Visibility::Private,
            "protected" => Visibility::Protected,
            _ => Visibility::Public,
        })
}

/// Override checks for one method of `class`: final parent, weaker
/// visibility, `static` mismatch, incompatible parameters.
fn overrides(index: &Index, class: &str, method: Node, src: &str, out: &mut Findings) {
    let Some(name) = method.child_by_field_name("name") else {
        return;
    };
    let Some(parent) = inherited(index, class, text(name, src)) else {
        return;
    };
    // Trait methods are copied, not inherited: the using class may change them.
    if class_symbol(index, &parent.fqn).is_some_and(|c| c.kind == Kind::Trait) {
        return;
    }
    let short = |fqn: &str| fqn.rsplit('\\').next().unwrap_or(fqn).to_string();
    let label = format!("{}::{}()", short(&parent.fqn), parent.name);
    if parent.is_final {
        out.add(name, Rule::OverridesFinalMethod, label);
        return;
    }
    let mut c = method.walk();
    let children: Vec<Node> = method.children(&mut c).collect();
    let visibility = written_visibility(method, src);
    if rank(visibility) > rank(parent.visibility) {
        let node = children
            .iter()
            .find(|n| n.kind() == "visibility_modifier")
            .copied()
            .unwrap_or(name);
        let wanted = match parent.visibility {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        };
        out.add(node, Rule::WeakerAccess, wanted);
    }
    let is_static = children.iter().any(|n| n.kind() == "static_modifier");
    if is_static != parent.is_static {
        out.add(name, Rule::StaticMismatch, label.clone());
    }
    let constructor = parent.name.eq_ignore_ascii_case("__construct");
    let parent_owner_kind = class_symbol(index, &parent.fqn).map(|c| c.kind);
    // Constructors only have to match abstract or interface declarations.
    if constructor && !parent.is_abstract && parent_owner_kind != Some(Kind::Interface) {
        return;
    }
    let own = crate::symbol::params_of_node(method, src);
    let required =
        |ps: &[crate::symbol::Param]| ps.iter().filter(|p| !p.optional && !p.variadic).count();
    let variadic = own.iter().any(|p| p.variadic);
    let accepts_all = required(&own) <= required(&parent.params)
        && (variadic || own.len() >= parent.params.len());
    if !accepts_all {
        out.add(name, Rule::SignatureMismatch, label);
    }
}

/// A constructor of a class whose parent has a constructor, never calling
/// `parent::__construct()`.
fn parent_constructor(index: &Index, class: Node, fqn: &str, src: &str, out: &mut Findings) {
    let Some(body) = class.child_by_field_name("body") else {
        return;
    };
    let mut c = body.walk();
    let Some(constructor) = body.named_children(&mut c).find(|m| {
        m.kind() == "method_declaration"
            && m.child_by_field_name("name")
                .is_some_and(|n| text(n, src).eq_ignore_ascii_case("__construct"))
    }) else {
        return;
    };
    let Some(parent) = class_symbol(index, fqn).and_then(|s| s.extends.clone()) else {
        return;
    };
    let parent_constructor = index
        .find_member(&parent, MemberTag::Method, "__construct")
        .into_iter()
        .map(|(_, s)| s)
        .find(|s| !s.is_abstract && !s.is_virtual && s.visibility != Visibility::Private);
    let calls_parent = constructor.child_by_field_name("body").is_some_and(|b| {
        text(b, src)
            .to_ascii_lowercase()
            .contains("parent::__construct")
    });
    if parent_constructor.is_some()
        && !calls_parent
        && let Some(name) = constructor.child_by_field_name("name")
    {
        out.add(name, Rule::MissingParentConstructorCall, "");
    }
}

/// `new` on something that cannot be instantiated.
fn instantiation(index: &Index, analyzer: &Analyzer, new: Node, src: &str, out: &mut Findings) {
    let Some(name) = new
        .named_child(0)
        .filter(|n| matches!(n.kind(), "name" | "qualified_name" | "relative_name"))
    else {
        return;
    };
    if matches!(
        text(name, src).to_ascii_lowercase().as_str(),
        "static" | "self" | "parent"
    ) {
        return;
    }
    let Some(Target::Class(fqn)) = analyzer.target_of(name) else {
        return;
    };
    let Some(sym) = class_symbol(index, &fqn) else {
        return;
    };
    if sym.kind != Kind::Class || sym.is_abstract {
        out.add(name, Rule::InstantiateAbstract, text(name, src));
    }
}

/// Whether a method call's receiver type is reliable enough to judge the
/// call: `$this`, a variable or a property. Results of other calls go
/// through return-type inference (conditional types, containers) and are
/// skipped.
fn reliable_receiver(call: Node) -> bool {
    let receiver = call
        .child_by_field_name("object")
        .or_else(|| call.child_by_field_name("scope"));
    receiver.is_none_or(|r| {
        matches!(
            r.kind(),
            "variable_name"
                | "member_access_expression"
                | "name"
                | "qualified_name"
                | "relative_name"
        )
    })
}

/// The function or method a call invokes, when the index knows exactly one
/// declaration (or several with the same parameters).
fn callee<'i>(index: &'i Index, analyzer: &Analyzer, call: Node) -> Option<&'i Symbol> {
    if !reliable_receiver(call) {
        return None;
    }
    let name = match call.kind() {
        "function_call_expression" => call
            .child_by_field_name("function")
            .filter(|f| matches!(f.kind(), "name" | "qualified_name"))?,
        "object_creation_expression" => call
            .named_child(0)
            .filter(|n| matches!(n.kind(), "name" | "qualified_name" | "relative_name"))?,
        _ => call
            .child_by_field_name("name")
            .filter(|n| n.kind() == "name")?,
    };
    let found: Vec<&Symbol> = match analyzer.target_of(name)? {
        Target::Function(candidates) => index
            .functions(&candidates)
            .into_iter()
            .map(|(_, s)| s)
            .collect(),
        Target::Member {
            class,
            tag: MemberTag::Method,
            name,
        } => {
            if has_magic(index, &class, &["__call", "__callStatic"])
                && !fully_indexed(index, &class)
            {
                return None;
            }
            index
                .find_member(&class, MemberTag::Method, &name)
                .into_iter()
                .map(|(_, s)| s)
                .collect()
        }
        Target::Class(fqn) => index
            .find_member(&fqn, MemberTag::Method, "__construct")
            .into_iter()
            .map(|(_, s)| s)
            .collect(),
        Target::Member { .. } => return None,
    };
    let first = *found.first()?;
    let consistent = found.iter().all(|s| s.params == first.params);
    (consistent && !first.is_virtual).then_some(first)
}

/// Calls with fewer arguments than required, or more than declared.
fn argument_count(index: &Index, analyzer: &Analyzer, call: Node, out: &mut Findings) {
    let Some(args_node) = call.child_by_field_name("arguments").or_else(|| {
        let mut c = call.walk();
        call.named_children(&mut c)
            .find(|n| n.kind() == "arguments")
    }) else {
        return;
    };
    let Some(args) = call_args_of(args_node) else {
        return;
    };
    let Some(sym) = callee(index, analyzer, call) else {
        return;
    };
    let required = sym
        .params
        .iter()
        .filter(|p| !p.optional && !p.variadic)
        .count();
    let variadic = sym.params.iter().any(|p| p.variadic);
    if args.len() < required {
        out.add(args_node, Rule::TooFewArguments, required.to_string());
    } else if !variadic && args.len() > sym.params.len() && !sym.reads_extra_args {
        out.add(
            args_node,
            Rule::TooManyArguments,
            sym.params.len().to_string(),
        );
    }
}

/// Argument expressions of an `arguments` node, `None` with named or
/// spread arguments.
fn call_args_of(args: Node) -> Option<Vec<Node>> {
    let mut c = args.walk();
    let mut out = Vec::new();
    for arg in args.named_children(&mut c) {
        if arg.kind() == "comment" {
            continue;
        }
        if arg.kind() != "argument"
            || arg.child_by_field_name("name").is_some()
            || arg
                .named_child(0)
                .is_some_and(|e| e.kind() == "variadic_unpacking")
        {
            return None;
        }
        out.push(arg);
    }
    Some(out)
}

/// Whether a declaration's return type is `void`, from its signature.
fn returns_void(sym: &Symbol) -> bool {
    sym.signature.rsplit_once(')').is_some_and(|(_, ret)| {
        ret.trim()
            .trim_start_matches(':')
            .trim()
            .eq_ignore_ascii_case("void")
    })
}

/// The value of a `void` call used as data.
fn void_result(index: &Index, analyzer: &Analyzer, call: Node, out: &mut Findings) {
    let used = call.parent().is_some_and(|p| {
        matches!(
            p.kind(),
            "assignment_expression"
                | "argument"
                | "return_statement"
                | "binary_expression"
                | "array_element_initializer"
        ) && !(p.kind() == "assignment_expression"
            && p.child_by_field_name("left")
                .is_some_and(|l| l.id() == call.id()))
    });
    if !used {
        return;
    }
    let Some(sym) = callee(index, analyzer, call) else {
        return;
    };
    if returns_void(sym) {
        out.add(call, Rule::VoidResultUsed, format!("{}()", sym.name));
    }
}

/// The class enclosing `node`, through closures (which run in their
/// class's scope).
fn scope_class(analyzer: &Analyzer, node: Node) -> Option<String> {
    analyzer.enclosing_class(node, &ctx_of(analyzer, node))
}

/// Whether `node` is inside a closure (whose `$this` may be rebound).
fn in_closure(node: Node) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            "anonymous_function" | "arrow_function" => return true,
            k if is_class_like(k) => return false,
            _ => cur = n.parent(),
        }
    }
    false
}

/// Whether `a` is `b` or a subtype of it.
fn is_subtype(index: &Index, a: &str, b: &str) -> bool {
    index.hierarchy(a).iter().any(|c| c.eq_ignore_ascii_case(b))
}

/// Private and protected members used out of their scope.
fn access(
    index: &Index,
    analyzer: &Analyzer,
    node: Node,
    name: Node,
    src: &str,
    out: &mut Findings,
) {
    let Some(Target::Member {
        class,
        tag,
        name: member,
    }) = analyzer.target_of(name)
    else {
        return;
    };
    let Some((_, sym)) = index.find_member(&class, tag, &member).into_iter().next() else {
        return;
    };
    // `$this->prop` naming an inherited private property creates a
    // dynamic property instead of failing.
    let on_this_property = tag == MemberTag::Property
        && node
            .child_by_field_name("object")
            .is_some_and(|o| text(o, src) == "$this");
    if sym.visibility == Visibility::Public
        || sym.is_virtual
        || in_closure(node)
        || on_this_property
    {
        return;
    }
    let magic: &[&str] = match tag {
        MemberTag::Method => &["__call", "__callStatic"],
        MemberTag::Property => &["__get", "__set"],
        MemberTag::Constant => &[],
    };
    if has_magic(index, &class, magic) {
        return;
    }
    let owner = &sym.fqn;
    let here = scope_class(analyzer, node);
    let allowed = here.as_deref().is_some_and(|here| match sym.visibility {
        Visibility::Private => {
            here.eq_ignore_ascii_case(owner)
                || class_symbol(index, owner).is_some_and(|o| o.kind == Kind::Trait)
                    && is_subtype(index, here, owner)
        }
        _ => is_subtype(index, here, owner) || is_subtype(index, owner, here),
    });
    if !allowed {
        let shown = match tag {
            MemberTag::Property => format!("${member}"),
            MemberTag::Method => format!("{member}()"),
            MemberTag::Constant => member.clone(),
        };
        out.add(name, Rule::InaccessibleMember, shown);
    }
}

/// `Foo::bar()` calling an instance method from outside `Foo`'s hierarchy.
fn static_call(index: &Index, analyzer: &Analyzer, call: Node, src: &str, out: &mut Findings) {
    let (Some(scope), Some(name)) = (
        call.child_by_field_name("scope"),
        call.child_by_field_name("name"),
    ) else {
        return;
    };
    if !matches!(scope.kind(), "name" | "qualified_name" | "relative_name")
        || matches!(
            text(scope, src).to_ascii_lowercase().as_str(),
            "parent" | "self" | "static"
        )
    {
        return;
    }
    let Some(Target::Member {
        class,
        name: method,
        ..
    }) = analyzer.target_of(name)
    else {
        return;
    };
    if has_magic(index, &class, &["__callStatic"]) || !fully_indexed(index, &class) {
        return;
    }
    let Some((_, sym)) = index
        .find_member(&class, MemberTag::Method, &method)
        .into_iter()
        .next()
    else {
        return;
    };
    let inside = scope_class(analyzer, call).is_some_and(|here| is_subtype(index, &here, &class));
    if !sym.is_static && !sym.is_virtual && !inside {
        out.add(
            name,
            Rule::StaticCallToInstanceMethod,
            format!("{}()", sym.name),
        );
    }
}

/// `Foo::BAR` naming no constant in a fully known hierarchy.
fn class_constant(index: &Index, analyzer: &Analyzer, access: Node, src: &str, out: &mut Findings) {
    let Some(name) = access.named_child(1).filter(|n| n.kind() == "name") else {
        return;
    };
    let Some(Target::Member {
        class,
        tag: MemberTag::Constant,
        name: constant,
    }) = analyzer.target_of(name)
    else {
        return;
    };
    if !fully_indexed(index, &class) || class.eq_ignore_ascii_case("stdClass") {
        return;
    }
    if index
        .find_member(&class, MemberTag::Constant, &constant)
        .is_empty()
    {
        let short = class.rsplit('\\').next().unwrap_or(&class);
        out.add(
            name,
            Rule::UndefinedClassConstant,
            format!("{short}::{}", text(name, src)),
        );
    }
}

/// Catch clauses whose type an earlier clause already catches.
fn catch_order(index: &Index, analyzer: &Analyzer, statement: Node, src: &str, out: &mut Findings) {
    let ctx = ctx_of(analyzer, statement);
    let mut caught: Vec<(String, String)> = Vec::new();
    let mut c = statement.walk();
    for clause in statement
        .named_children(&mut c)
        .filter(|n| n.kind() == "catch_clause")
    {
        let Some(types) = clause.child_by_field_name("type") else {
            continue;
        };
        let mut tc = types.walk();
        let names: Vec<Node> = types
            .named_children(&mut tc)
            .map(|n| {
                if n.kind() == "named_type" {
                    n.named_child(0).unwrap_or(n)
                } else {
                    n
                }
            })
            .filter(|n| matches!(n.kind(), "name" | "qualified_name"))
            .collect();
        for n in &names {
            let fqn = ctx.resolve_class(text(*n, src));
            let hierarchy = index.hierarchy(&fqn);
            if let Some((_, written)) = caught.iter().find(|(earlier, _)| {
                !earlier.eq_ignore_ascii_case(&fqn)
                    && hierarchy.iter().any(|h| h.eq_ignore_ascii_case(earlier))
            }) {
                out.add(*n, Rule::CatchOrder, written.clone());
            }
        }
        for n in names {
            caught.push((ctx.resolve_class(text(n, src)), text(n, src).to_string()));
        }
    }
}

/// A `match` listing cases of one enum, without `default`, that misses
/// some of them.
fn enum_match(index: &Index, analyzer: &Analyzer, expr: Node, src: &str, out: &mut Findings) {
    let Some(block) = expr.child_by_field_name("body").or_else(|| {
        let mut c = expr.walk();
        expr.named_children(&mut c)
            .find(|n| n.kind() == "match_block")
    }) else {
        return;
    };
    let mut enum_class: Option<String> = None;
    let mut covered = HashSet::new();
    let mut c = block.walk();
    for arm in block.named_children(&mut c) {
        match arm.kind() {
            "match_default_expression" => return,
            "match_conditional_expression" => {
                let Some(list) = arm.child_by_field_name("conditional_expressions") else {
                    return;
                };
                let mut lc = list.walk();
                for cond in list.named_children(&mut lc) {
                    if cond.kind() != "class_constant_access_expression" {
                        return;
                    }
                    let Some(name) = cond.named_child(1) else {
                        return;
                    };
                    let Some(Target::Member {
                        class,
                        tag: MemberTag::Constant,
                        ..
                    }) = analyzer.target_of(name)
                    else {
                        return;
                    };
                    if !enum_class
                        .get_or_insert_with(|| class.clone())
                        .eq_ignore_ascii_case(&class)
                    {
                        return;
                    }
                    covered.insert(text(name, src).to_string());
                }
            }
            _ => {}
        }
    }
    let Some(class) = enum_class else { return };
    if class_symbol(index, &class).is_none_or(|s| s.kind != Kind::Enum) {
        return;
    }
    let missing: Vec<String> = index
        .members(&class)
        .into_iter()
        .filter(|(_, s)| s.kind == Kind::EnumCase && !covered.contains(&s.name))
        .map(|(_, s)| s.name.clone())
        .collect();
    if !missing.is_empty() {
        let node = expr.child(0).unwrap_or(expr);
        out.add(node, Rule::EnumMatchNotExhaustive, missing.join(", "));
    }
}

/// `$obj->prop = …` on a readonly property, outside its class.
fn readonly_write(
    index: &Index,
    analyzer: &Analyzer,
    assignment: Node,
    src: &str,
    out: &mut Findings,
) {
    let Some(left) = assignment
        .child_by_field_name("left")
        .filter(|l| l.kind() == "member_access_expression")
    else {
        return;
    };
    let Some(name) = left.child_by_field_name("name") else {
        return;
    };
    let Some(Target::Member {
        class,
        tag: MemberTag::Property,
        name: prop,
    }) = analyzer.target_of(name)
    else {
        return;
    };
    let Some((_, sym)) = index
        .find_member(&class, MemberTag::Property, &prop)
        .into_iter()
        .next()
    else {
        return;
    };
    let readonly = sym.is_readonly || class_symbol(index, &sym.fqn).is_some_and(|c| c.is_readonly);
    let inside =
        scope_class(analyzer, assignment).is_some_and(|h| h.eq_ignore_ascii_case(&sym.fqn));
    if readonly && !inside {
        out.add(
            name,
            Rule::ReadonlyModified,
            format!("${}", text(name, src)),
        );
    }
}

/// `@dataProvider name` / `#[DataProvider('name')]` naming a method the
/// test class does not have.
fn data_provider(index: &Index, analyzer: &Analyzer, method: Node, src: &str, out: &mut Findings) {
    let mut providers: Vec<(usize, String)> = Vec::new();
    if let Some(doc) = doc_comment(method, src) {
        let doc_text = text(doc, src);
        for (i, _) in doc_text.match_indices("@dataProvider") {
            let rest = &doc_text[i + 13..];
            let skipped = rest.len() - rest.trim_start().len();
            let name: String = rest
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                providers.push((doc.start_byte() + i + 13 + skipped, name));
            }
        }
    }
    if let Some(attrs) = method.child_by_field_name("attributes") {
        let attr_text = text(attrs, src);
        for (i, _) in attr_text.match_indices("DataProvider(") {
            let rest = &attr_text[i + 13..];
            let quote = rest.find(['\'', '"']);
            if let Some(q) = quote {
                let name: String = rest[q + 1..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    providers.push((attrs.start_byte() + i + 13 + q + 1, name));
                }
            }
        }
    }
    if providers.is_empty() {
        return;
    }
    let Some(class) = scope_class(analyzer, method) else {
        return;
    };
    let class_body = method.parent();
    for (start, name) in providers {
        let in_file = class_body.is_some_and(|b| {
            let mut c = b.walk();
            b.named_children(&mut c).any(|m| {
                m.kind() == "method_declaration"
                    && m.child_by_field_name("name")
                        .is_some_and(|n| text(n, src).eq_ignore_ascii_case(&name))
            })
        });
        let indexed = !index
            .find_member(&class, MemberTag::Method, &name)
            .is_empty();
        if !in_file && !indexed && fully_indexed(index, &class) {
            out.add_range(start, start + name.len(), Rule::UndefinedDataProvider, name);
        }
    }
}

/// Fixes that need the index: implementing missing methods and restoring
/// the parent's visibility.
///
/// Called by [`quick_fixes`] before the syntax-only [`rule_fix`].
pub(crate) fn semantic_fix(
    index: &Index,
    rule: Rule,
    src: &str,
    start: usize,
    detail: &str,
) -> Option<Vec<Edit>> {
    match rule {
        Rule::MissingImplementation => {
            let tree = parse(src);
            let mut class = tree.root_node().descendant_for_byte_range(start, start)?;
            while !is_class_like(class.kind()) {
                class = class.parent()?;
            }
            let name = class.child_by_field_name("name")?;
            let ctx = crate::syntax::context_at(tree.root_node(), src, start);
            let fqn = ctx.qualify(text(name, src));
            let body = class.child_by_field_name("body")?;
            let stubs: String = missing_implementations(index, &fqn)
                .iter()
                .map(implementation_stub)
                .collect();
            (!stubs.is_empty()).then(|| {
                vec![Edit {
                    path: None,
                    start: body.end_byte() - 1,
                    end: body.end_byte() - 1,
                    text: stubs,
                }]
            })
        }
        Rule::WeakerAccess => {
            let tree = parse(src);
            let node = tree.root_node().descendant_for_byte_range(start, start)?;
            let modifier = if node.kind() == "visibility_modifier" {
                node
            } else {
                node.parent()
                    .filter(|p| p.kind() == "visibility_modifier")?
            };
            Some(vec![Edit {
                path: None,
                start: modifier.start_byte(),
                end: modifier.end_byte(),
                text: detail.to_string(),
            }])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::index_source;
    use std::path::PathBuf;

    fn index_with(files: &[(&str, &str)]) -> Index {
        let mut index = Index::default();
        for (path, src) in files {
            index.update(PathBuf::from(path), index_source(src));
        }
        index
    }

    /// Rules fired on `src`, which is also indexed as `/main.php`.
    fn rules(lib: &str, src: &str) -> Vec<Rule> {
        let index = index_with(&[("/lib.php", lib), ("/main.php", src)]);
        let tree = crate::syntax::parse(src);
        let analyzer = Analyzer::new(&index, src, &tree);
        let mut out = Findings::default();
        semantic_checks(&index, &analyzer, tree.root_node(), src, &mut out);
        out.list.iter().map(|f| f.rule).collect()
    }

    const LIB: &str = "<?php namespace L;
        interface Shape { public function area(): float; }
        abstract class Base { abstract protected function name(): string; public function hello(int $a, $b = 1) {} final public function locked() {} public static function make() {} private function secret() {} protected $guarded; public readonly int $id; }
        final class Sealed {}
        trait T {}
        enum Suit { case Hearts; case Spades; }
        class Logger { public function log(string $m): void {} public function __construct(int $level) {} }
        class Child extends \\Exception {}";

    #[test]
    fn concrete_class_should_implement_interface_methods() {
        assert_eq!(
            rules(LIB, "<?php namespace L; class Square implements Shape {}"),
            [Rule::MissingImplementation]
        );
    }

    #[test]
    fn implemented_methods_should_be_accepted() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; class Square implements Shape { public function area(): float { return 1.0; } }"
            ),
            []
        );
    }

    #[test]
    fn extending_final_class_should_be_an_error() {
        assert_eq!(
            rules(LIB, "<?php namespace L; class X extends Sealed {}"),
            [Rule::ExtendsFinalClass]
        );
    }

    #[test]
    fn implementing_a_trait_should_be_an_error() {
        assert_eq!(
            rules(LIB, "<?php namespace L; class X implements T {}"),
            [Rule::WrongSupertypeKind]
        );
    }

    #[test]
    fn overriding_final_method_should_be_an_error() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; class X extends Base { protected function name(): string { return ''; } public function locked() {} }"
            ),
            [Rule::OverridesFinalMethod]
        );
    }

    #[test]
    fn weaker_visibility_should_be_an_error() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; class X extends Base { protected function name(): string { return ''; } protected function hello(int $a, $b = 1) {} }"
            ),
            [Rule::WeakerAccess]
        );
    }

    #[test]
    fn override_requiring_more_parameters_should_be_incompatible() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; class X extends Base { protected function name(): string { return ''; } public function hello(int $a, $b, $c) {} }"
            ),
            [Rule::SignatureMismatch]
        );
    }

    #[test]
    fn instantiating_abstract_class_should_be_an_error() {
        assert_eq!(
            rules(LIB, "<?php namespace L; $b = new Base();"),
            [Rule::InstantiateAbstract]
        );
    }

    #[test]
    fn too_few_arguments_should_be_an_error() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; function f(Logger $l) { $l->log(); }"
            ),
            [Rule::TooFewArguments]
        );
    }

    #[test]
    fn constructor_arguments_should_be_counted() {
        assert_eq!(
            rules(LIB, "<?php namespace L; $l = new Logger();"),
            [Rule::TooFewArguments]
        );
    }

    #[test]
    fn too_many_arguments_should_be_a_warning() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; function f(Logger $l) { $l->log('a', 'b'); }"
            ),
            [Rule::TooManyArguments]
        );
    }

    #[test]
    fn void_result_used_should_be_flagged() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; function f(Logger $l) { $x = $l->log('a'); }"
            ),
            [Rule::VoidResultUsed]
        );
    }

    #[test]
    fn private_method_from_outside_should_be_inaccessible() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; function f(Base $b) { $b->secret(); }"
            ),
            [Rule::InaccessibleMember]
        );
    }

    #[test]
    fn protected_property_in_subclass_should_be_accessible() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; abstract class X extends Base { function g() { return $this->guarded; } }"
            ),
            []
        );
    }

    #[test]
    fn static_call_to_instance_method_should_be_an_error() {
        assert_eq!(
            rules(LIB, "<?php namespace L; Logger::log('x');"),
            [Rule::StaticCallToInstanceMethod]
        );
    }

    #[test]
    fn undefined_enum_case_should_be_an_error() {
        assert_eq!(
            rules(LIB, "<?php namespace L; $s = Suit::Clubs;"),
            [Rule::UndefinedClassConstant]
        );
    }

    #[test]
    fn catch_after_parent_catch_should_be_flagged() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; try { f(); } catch (\\Exception $e) {} catch (Child $c) {}"
            ),
            [Rule::CatchOrder]
        );
    }

    #[test]
    fn match_missing_enum_case_should_be_flagged() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; function f(Suit $s) { return match ($s) { Suit::Hearts => 1 }; }"
            ),
            [Rule::EnumMatchNotExhaustive]
        );
    }

    #[test]
    fn readonly_write_outside_class_should_be_an_error() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; function f(Base $b) { $b->id = 2; }"
            ),
            [Rule::ReadonlyModified]
        );
    }

    #[test]
    fn constructor_without_parent_call_should_be_flagged() {
        assert_eq!(
            rules(
                LIB,
                "<?php namespace L; class Verbose extends Logger { public function __construct() { } }"
            ),
            [Rule::MissingParentConstructorCall]
        );
    }

    #[test]
    fn missing_data_provider_should_be_an_error() {
        assert_eq!(
            rules(
                "<?php class TestCase {}",
                "<?php class FooTest extends TestCase {\n/** @dataProvider cases */\npublic function testX($a) {}\n}"
            ),
            [Rule::UndefinedDataProvider]
        );
    }
}
