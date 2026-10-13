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
            conditional: None,
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
        // `Builder<static>`: bound to the model the scope is called on.
        let builder = Ty {
            class: Some("Illuminate\\Database\\Eloquent\\Builder".to_string()),
            args: vec![STATIC_TYPE.to_string()],
            ..Ty::default()
        };
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

/// Prefix of the pseudo class that owns a database table's columns
/// (`<table>users`). Migrations declare columns as its properties and a
/// model lists it among its supertypes, so member lookups on the model
/// find the columns. Never a valid PHP name, so it cannot clash.
pub const TABLE_PREFIX: &str = "<table>";

/// The pseudo class owning the columns of `table`.
pub fn table_owner(table: &str) -> String {
    format!("{TABLE_PREFIX}{}", table.to_ascii_lowercase())
}

/// Parent classes (short names) that make a class an Eloquent model.
const MODEL_BASES: &[&str] = &["model", "authenticatable", "pivot", "morphpivot", "user"];

/// Properties and methods only a model declares.
const MODEL_MARKERS: &[&str] = &["$fillable", "$guarded", "$casts", "$table", "$connection"];

/// English plural of the last word of a snake_case name, as Laravel's
/// pluralizer does for regular nouns (`user` -> `users`, `category` ->
/// `categories`); a word already ending in `s` is taken as plural.
fn plural(snake: &str) -> String {
    let (head, word) = snake.rsplit_once('_').map_or(("", snake), |(h, w)| (h, w));
    let consonant_y = word.len() > 1
        && word.ends_with('y')
        && !word[..word.len() - 1].ends_with(['a', 'e', 'i', 'o', 'u']);
    let word = if consonant_y {
        format!("{}ies", &word[..word.len() - 1])
    } else if word.ends_with("ss")
        || word.ends_with(['x', 'z'])
        || word.ends_with("ch")
        || word.ends_with("sh")
    {
        format!("{word}es")
    } else if word.ends_with('s') {
        word.to_string()
    } else {
        format!("{word}s")
    };
    if head.is_empty() {
        word
    } else {
        format!("{head}_{word}")
    }
}

/// Database table of the class declared by `class` when it looks like an
/// Eloquent model (extends `Model`, `Authenticatable`, `Pivot`…, or
/// declares `$fillable`/`$table`/`$casts`…): its `$table` value, else the
/// plural snake_case of its name (`BlogPost` -> `blog_posts`). `None` for
/// other classes.
pub fn model_table(class: Node, src: &str) -> Option<String> {
    let name = text(class.child_by_field_name("name")?, src);
    let body = class.child_by_field_name("body")?;
    let base = class
        .named_children(&mut class.walk())
        .find(|n| n.kind() == "base_clause")
        .map(|b| {
            text(b, src)
                .rsplit(['\\', ' '])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
        });
    let mut table = None;
    let mut marked = false;
    for member in body.named_children(&mut body.walk()) {
        if member.kind() != "property_declaration" {
            continue;
        }
        for el in member.named_children(&mut member.walk()) {
            let Some(var) = el.child_by_field_name("name") else {
                continue;
            };
            let var = text(var, src);
            marked |= MODEL_MARKERS.contains(&var);
            if var == "$table" {
                table = el
                    .child_by_field_name("default_value")
                    .and_then(|v| string_value(v, src));
            }
        }
    }
    let is_model = marked || base.is_some_and(|b| MODEL_BASES.contains(&b.as_str()));
    is_model.then(|| table.unwrap_or_else(|| plural(&snake_case(name))))
}

/// A table column declared by a migration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// Table the column belongs to.
    pub table: String,
    /// Column name.
    pub name: String,
    /// PHP type of the attribute, for display (`int`, `string`, `Carbon`).
    pub php_type: &'static str,
    /// Attribute type (Carbon for timestamps), empty for scalars.
    pub ty: Ty,
    /// Byte range of the declaring call, for navigation.
    pub start: usize,
    /// End of that range.
    pub end: usize,
}

/// Carbon, the type Eloquent gives timestamp attributes.
const CARBON: &str = "Carbon\\Carbon";

/// Columns a `$table->method(...)` Blueprint call adds, as (name, PHP
/// type, is a timestamp); `first` is the first string argument and
/// `class_arg` the class of a `Foo::class` first argument.
fn blueprint_columns(
    method: &str,
    first: Option<String>,
    class_arg: Option<&str>,
) -> Vec<(String, &'static str, bool)> {
    let named = |default: &str| first.clone().unwrap_or_else(|| default.to_string());
    let one = |ty: &'static str| {
        first
            .clone()
            .map(|n| vec![(n, ty, false)])
            .unwrap_or_default()
    };
    match method.to_ascii_lowercase().as_str() {
        "id" => vec![(named("id"), "int", false)],
        "timestamps" | "nullabletimestamps" | "timestampstz" => vec![
            ("created_at".into(), "Carbon", true),
            ("updated_at".into(), "Carbon", true),
        ],
        "softdeletes" | "softdeletestz" => vec![(named("deleted_at"), "Carbon", true)],
        "remembertoken" => vec![("remember_token".into(), "string", false)],
        "morphs" | "nullablemorphs" | "uuidmorphs" | "nullableuuidmorphs" | "ulidmorphs"
        | "nullableulidmorphs" => first
            .map(|n| {
                vec![
                    (format!("{n}_id"), "int|string", false),
                    (format!("{n}_type"), "string", false),
                ]
            })
            .unwrap_or_default(),
        "foreignidfor" => class_arg
            .map(|c| {
                let short = c.rsplit('\\').next().unwrap_or(c);
                vec![(format!("{}_id", snake_case(short)), "int", false)]
            })
            .unwrap_or_default(),
        "increments"
        | "bigincrements"
        | "smallincrements"
        | "tinyincrements"
        | "mediumincrements"
        | "integer"
        | "biginteger"
        | "smallinteger"
        | "tinyinteger"
        | "mediuminteger"
        | "unsignedinteger"
        | "unsignedbiginteger"
        | "unsignedsmallinteger"
        | "unsignedtinyinteger"
        | "unsignedmediuminteger"
        | "foreignid"
        | "year" => one("int"),
        "decimal" | "double" | "float" | "unsigneddecimal" => one("float"),
        "boolean" => one("bool"),
        "json" | "jsonb" => one("array"),
        "timestamp" | "timestamptz" | "datetime" | "datetimetz" => first
            .map(|n| vec![(n, "string", false)])
            .unwrap_or_default(),
        "string" | "char" | "text" | "tinytext" | "mediumtext" | "longtext" | "uuid" | "ulid"
        | "foreignuuid" | "foreignulid" | "enum" | "set" | "date" | "time" | "timetz"
        | "binary" | "ipaddress" | "macaddress" | "geometry" | "geography" | "vector" => {
            one("string")
        }
        _ => Vec::new(),
    }
}

/// Columns declared by `Schema::create` / `Schema::table` calls anywhere in
/// a file (migrations): each `$table->type('name')` call on the closure's
/// Blueprint parameter. Dropped and renamed columns are not tracked.
pub fn migration_columns(root: Node, src: &str) -> Vec<Column> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        stack.extend(node.named_children(&mut node.walk()));
        if node.kind() != "scoped_call_expression" {
            continue;
        }
        let scope = node.child_by_field_name("scope").map(|s| text(s, src));
        let method = node.child_by_field_name("name").map(|n| text(n, src));
        if !scope.is_some_and(|s| s.rsplit('\\').next() == Some("Schema"))
            || !method.is_some_and(|m| {
                m.eq_ignore_ascii_case("create") || m.eq_ignore_ascii_case("table")
            })
        {
            continue;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            continue;
        };
        let values: Vec<Node> = args
            .named_children(&mut args.walk())
            .filter_map(|a| a.named_child(0))
            .collect();
        let (Some(table), Some(closure)) = (
            values.first().and_then(|v| string_value(*v, src)),
            values
                .get(1)
                .filter(|v| matches!(v.kind(), "anonymous_function" | "arrow_function")),
        ) else {
            continue;
        };
        let Some(param) = closure
            .child_by_field_name("parameters")
            .and_then(|p| p.named_child(0))
            .and_then(|p| p.child_by_field_name("name"))
            .map(|n| text(n, src))
        else {
            continue;
        };
        let mut inner = vec![*closure];
        while let Some(n) = inner.pop() {
            inner.extend(n.named_children(&mut n.walk()));
            if n.kind() != "member_call_expression"
                || !n
                    .child_by_field_name("object")
                    .is_some_and(|o| text(o, src) == param)
            {
                continue;
            }
            let Some(name) = n.child_by_field_name("name") else {
                continue;
            };
            let first_arg = first_argument(n);
            let first = first_arg.and_then(|a| string_value(a, src));
            let class_arg = first_arg.and_then(|a| text(a, src).strip_suffix("::class"));
            for (column, php_type, is_time) in blueprint_columns(text(name, src), first, class_arg)
            {
                out.push(Column {
                    table: table.clone(),
                    name: column,
                    php_type,
                    ty: if is_time {
                        Ty::class(CARBON)
                    } else {
                        Ty::default()
                    },
                    start: n.start_byte(),
                    end: n.end_byte(),
                });
            }
        }
    }
    out
}
