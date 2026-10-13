//! Eloquent conventions: the members a Laravel model gets from its own
//! declarations. A relation method `posts()` also reads as a `$posts`
//! property, `scopeActive()` as `active()`, `getFullNameAttribute()` and
//! `fullName(): Attribute` as `$full_name`, and `$fillable` / `$casts`
//! entries as attributes. Applied to every class; the patterns are
//! specific enough not to misfire elsewhere.

use super::*;
use crate::syntax::{NameContext, text};
use tree_sitter::Node;

/// Relations returning one model.
const SINGLE_RELATIONS: &[&str] = &["hasone", "belongsto", "morphone", "hasonethrough"];

/// Relations returning a collection of models.
const MANY_RELATIONS: &[&str] = &[
    "hasmany",
    "belongstomany",
    "morphmany",
    "morphtomany",
    "morphedbymany",
    "hasmanythrough",
];

/// Eloquent's collection, the type of to-many relation properties.
const ELOQUENT_COLLECTION: &str = "Illuminate\\Database\\Eloquent\\Collection";

/// `FullName` -> `full_name`.
fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `Active` -> `active`.
fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|c| c.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Contents of a string literal node, quotes removed.
fn string_value(node: Node, src: &str) -> Option<String> {
    matches!(node.kind(), "string" | "encapsed_string").then(|| {
        text(node, src)
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string()
    })
}

/// `X::class` -> the resolved FQN of `X`.
fn class_constant(node: Node, src: &str, ctx: &NameContext) -> Option<String> {
    if node.kind() != "class_constant_access_expression" {
        return None;
    }
    let raw = text(node, src);
    let class = raw.strip_suffix("::class")?;
    Some(ctx.resolve_class(class))
}

/// First argument expression of a call node.
fn first_argument(call: Node) -> Option<Node> {
    let args = call.child_by_field_name("arguments")?;
    let arg = args.named_child(0)?;
    if arg.kind() == "argument" {
        arg.named_child(0)
    } else {
        Some(arg)
    }
}

/// The first `$this->hasMany(X::class …)`-style call in `body`: relation
/// name (lowercased) and related model.
fn relation_call(body: Node, src: &str, ctx: &NameContext) -> Option<(String, Option<String>)> {
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if node.kind() == "member_call_expression"
            && node
                .child_by_field_name("object")
                .is_some_and(|o| text(o, src) == "$this")
        {
            let name = node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_ascii_lowercase())
                .unwrap_or_default();
            if SINGLE_RELATIONS.contains(&name.as_str())
                || MANY_RELATIONS.contains(&name.as_str())
                || name == "morphto"
            {
                let model = first_argument(node).and_then(|a| class_constant(a, src, ctx));
                return Some((name, model));
            }
        }
        let mut c = node.walk();
        let children: Vec<Node> = node.named_children(&mut c).collect();
        stack.extend(children.into_iter().rev());
    }
    None
}

/// Type of a relation property.
fn relation_type(relation: &str, model: Option<String>) -> Ty {
    match model {
        Some(model) if MANY_RELATIONS.contains(&relation) => Ty {
            class: Some(ELOQUENT_COLLECTION.to_string()),
            elem: Some(model.clone()),
            args: vec![String::new(), model],
        },
        Some(model) => Ty::class(model),
        None => Ty::default(),
    }
}

/// Type of a cast attribute from its cast spec (`'datetime'`, `Status::class`).
fn cast_type(value: Node, src: &str, ctx: &NameContext) -> Ty {
    if let Some(class) = class_constant(value, src, ctx) {
        return Ty::class(class);
    }
    let spec = string_value(value, src).unwrap_or_default();
    let base = spec.split(':').next().unwrap_or_default();
    match base {
        "date" | "datetime" | "timestamp" | "custom_datetime" => Ty::class("Carbon\\Carbon"),
        "immutable_date" | "immutable_datetime" | "immutable_custom_datetime" => {
            Ty::class("Carbon\\CarbonImmutable")
        }
        "collection" => Ty::class("Illuminate\\Support\\Collection"),
        _ => Ty::default(),
    }
}

/// Entries of the first array literal under `node`: `(key, value)` with
/// `key` `None` for list entries.
fn array_entries<'t>(node: Node<'t>) -> Vec<(Option<Node<'t>>, Node<'t>)> {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "array_creation_expression" {
            let mut c = n.walk();
            return n
                .named_children(&mut c)
                .filter(|e| e.kind() == "array_element_initializer")
                .filter_map(|e| match (e.named_child(0), e.named_child(1)) {
                    (Some(k), Some(v)) => Some((Some(k), v)),
                    (Some(v), None) => Some((None, v)),
                    _ => None,
                })
                .collect();
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    Vec::new()
}

/// Attribute properties from a casts array literal under `node`.
fn cast_members(node: Node, src: &str, ctx: &NameContext) -> Vec<ConventionMember> {
    array_entries(node)
        .into_iter()
        .filter_map(|(key, value)| {
            let key = key?;
            Some(ConventionMember {
                kind: Kind::Property,
                name: string_value(key, src)?,
                ty: cast_type(value, src, ctx),
                is_static: false,
                start: key.start_byte(),
                end: key.end_byte(),
            })
        })
        .collect()
}

/// Members a method adds by convention: a relation property, a scope, an
/// accessor attribute, or the attributes of `casts()`. `return_ty` is the
/// method's resolved return type.
pub fn eloquent_members_of_method(
    method: Node,
    src: &str,
    ctx: &NameContext,
    return_ty: &Ty,
) -> Vec<ConventionMember> {
    let (Some(name_node), Some(body)) = (
        method.child_by_field_name("name"),
        method.child_by_field_name("body"),
    ) else {
        return Vec::new();
    };
    let name = text(name_node, src);
    let member = |kind, name: String, ty: Ty, is_static| ConventionMember {
        kind,
        name,
        ty,
        is_static,
        start: name_node.start_byte(),
        end: name_node.end_byte(),
    };
    if name.eq_ignore_ascii_case("casts") {
        return cast_members(body, src, ctx);
    }
    if let Some(scope) = name
        .strip_prefix("scope")
        .filter(|s| s.starts_with(|c: char| c.is_ascii_uppercase()))
    {
        let builder = Ty::class("Illuminate\\Database\\Eloquent\\Builder");
        return vec![member(Kind::Method, lower_first(scope), builder, true)];
    }
    if let Some(attr) = name
        .strip_prefix("get")
        .and_then(|n| n.strip_suffix("Attribute"))
        .filter(|n| n.starts_with(|c: char| c.is_ascii_uppercase()))
    {
        return vec![member(
            Kind::Property,
            snake_case(attr),
            return_ty.clone(),
            false,
        )];
    }
    let returns_attribute = method.child_by_field_name("return_type").is_some_and(|t| {
        text(t, src)
            .trim_start_matches('\\')
            .ends_with("Casts\\Attribute")
            || text(t, src) == "Attribute"
    });
    if returns_attribute {
        return vec![member(
            Kind::Property,
            snake_case(name),
            Ty::default(),
            false,
        )];
    }
    match relation_call(body, src, ctx) {
        Some((relation, model)) => vec![member(
            Kind::Property,
            name.to_string(),
            relation_type(&relation, model),
            false,
        )],
        None => Vec::new(),
    }
}

/// Attribute properties from `$fillable` and `$casts` declarations.
pub fn eloquent_members_of_property(
    decl: Node,
    src: &str,
    ctx: &NameContext,
) -> Vec<ConventionMember> {
    let mut c = decl.walk();
    let Some(element) = decl
        .named_children(&mut c)
        .find(|n| n.kind() == "property_element")
    else {
        return Vec::new();
    };
    let Some(name) = element.child_by_field_name("name") else {
        return Vec::new();
    };
    let Some(value) = element.child_by_field_name("default_value") else {
        return Vec::new();
    };
    match text(name, src) {
        "$casts" => cast_members(value, src, ctx),
        "$fillable" => array_entries(value)
            .into_iter()
            .filter_map(|(_, v)| {
                Some(ConventionMember {
                    kind: Kind::Property,
                    name: string_value(v, src)?,
                    ty: Ty::default(),
                    is_static: false,
                    start: v.start_byte(),
                    end: v.end_byte(),
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}
