//! Eloquent strings: column names in query calls (`User::where('email')`,
//! `->orderBy('created_at')`, `create(['name' => …])`, `$fillable`) and
//! relation names (`->with('posts.comments')`, `whereHas('roles')`),
//! completed from the model's migration columns and relation methods, and
//! resolved to their declarations. The model comes from the call's
//! receiver type, so it works on models, builders and relations alike.

use super::is_model;
use crate::analysis::{Analyzer, Candidate};
use crate::index::Index;
use crate::symbol::{Kind, Symbol, TABLE_PREFIX, Ty};
use crate::syntax::{Pos, parse, text};
use std::path::PathBuf;
use tree_sitter::Node;

/// Calls whose first argument is a column name (lowercase).
const COLUMN_CALLS: &[&str] = &[
    "where",
    "orwhere",
    "wherenot",
    "wherein",
    "orwherein",
    "wherenotin",
    "wherenull",
    "orwherenull",
    "wherenotnull",
    "wherebetween",
    "wherenotbetween",
    "wheredate",
    "wheremonth",
    "whereyear",
    "whereday",
    "wheretime",
    "wherecolumn",
    "wherelike",
    "firstwhere",
    "orderby",
    "orderbydesc",
    "latest",
    "oldest",
    "groupby",
    "having",
    "pluck",
    "value",
    "increment",
    "decrement",
    "sum",
    "avg",
    "average",
    "max",
    "min",
    "select",
    "addselect",
];

/// Calls taking a list of columns (`select(['id', 'name'])`).
const COLUMN_LIST_CALLS: &[&str] = &[
    "select",
    "addselect",
    "get",
    "only",
    "except",
    "makehidden",
    "makevisible",
    "groupby",
];

/// Calls taking an array keyed by column (`create(['name' => …])`).
const COLUMN_KEY_CALLS: &[&str] = &[
    "create",
    "forcecreate",
    "make",
    "update",
    "fill",
    "forcefill",
    "firstorcreate",
    "firstornew",
    "updateorcreate",
    "insert",
    "where",
];

/// Calls whose first argument (or its list) names relations.
const RELATION_CALLS: &[&str] = &[
    "with",
    "without",
    "load",
    "loadmissing",
    "loadcount",
    "has",
    "orhas",
    "wherehas",
    "orwherehas",
    "doesnthave",
    "wheredoesnthave",
    "withcount",
    "withexists",
    "whererelation",
];

/// Model properties whose array lists columns (values) or maps them (keys).
const COLUMN_PROPERTIES: &[&str] = &[
    "$fillable",
    "$guarded",
    "$hidden",
    "$visible",
    "$casts",
    "$dates",
];

/// Namespace of Eloquent's relation classes.
const RELATIONS_NAMESPACE: &str = "Illuminate\\Database\\Eloquent\\Relations\\";

/// What the string at the caret names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wanted {
    /// A column of the model.
    Column,
    /// A relation, possibly dotted (`posts.comments`).
    Relation,
}

/// An Eloquent string at the caret: the model it belongs to, what it names,
/// and the byte range of the part being completed.
struct Context {
    /// FQN of the model.
    model: String,
    /// Column or relation.
    wanted: Wanted,
    /// Start of the completed part (after the last `.` for relations).
    start: usize,
    /// The whole string content.
    value: String,
    /// Byte offset where the string content starts.
    content_start: usize,
}

/// Completion inside an Eloquent string: where the replaced text starts and
/// the model's columns or relations. `None` outside such strings.
pub fn eloquent_completions(
    index: &Index,
    src: &str,
    offset: usize,
) -> Option<(usize, Vec<Candidate>)> {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    let ctx = context(&analyzer, src, offset)?;
    let items = match ctx.wanted {
        Wanted::Column => columns(index, &ctx.model)
            .into_iter()
            .map(|(_, s)| Candidate {
                label: s.name.clone(),
                kind: Kind::Property,
                detail: column_detail(&s),
                insert_text: s.name.clone(),
                import: None,
            })
            .collect(),
        Wanted::Relation => {
            // `posts.comm|`: the relations of the model `posts` leads to.
            let typed = &ctx.value[..ctx.start - ctx.content_start];
            let model = typed
                .split('.')
                .filter(|s| !s.is_empty())
                .try_fold(ctx.model.clone(), |model, name| {
                    related_model(index, &model, name)
                })?;
            relations(index, &model)
                .into_iter()
                .map(|(s, related)| Candidate {
                    label: s.name.clone(),
                    kind: Kind::Method,
                    detail: related.unwrap_or(s.signature),
                    insert_text: s.name,
                    import: None,
                })
                .collect()
        }
    };
    Some((ctx.start, items))
}

/// Whether the caret is inside an Eloquent string (completion stays open
/// while typing there).
pub fn in_eloquent_string(index: &Index, src: &str, offset: usize) -> bool {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    context(&analyzer, src, offset).is_some()
}

/// Declaration of the column or relation named by the Eloquent string at
/// `offset`: the migration line declaring the column, or the relation
/// method. For dotted relations, the segment under the caret.
pub fn eloquent_definition(index: &Index, src: &str, offset: usize) -> Option<(PathBuf, Pos)> {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    let ctx = context(&analyzer, src, offset)?;
    match ctx.wanted {
        Wanted::Column => {
            let name = ctx.value.split([' ', '.']).next()?;
            columns(index, &ctx.model)
                .into_iter()
                .find(|(_, s)| s.name == name)
                .map(|(p, s)| (p, s.name_span.start))
        }
        Wanted::Relation => {
            let caret = offset - ctx.content_start;
            let mut model = ctx.model.clone();
            let mut at = 0;
            for name in ctx.value.split('.') {
                if caret <= at + name.len() {
                    return relation_method(index, &model, name)
                        .map(|(p, s)| (p, s.name_span.start));
                }
                model = related_model(index, &model, name)?;
                at += name.len() + 1;
            }
            None
        }
    }
}

/// Recognises the Eloquent string around `offset`.
fn context(analyzer: &Analyzer, src: &str, offset: usize) -> Option<Context> {
    let string = string_at(analyzer.tree.root_node(), offset)?;
    let content_start = string.start_byte() + 1;
    let content_end = string.end_byte().saturating_sub(1).max(content_start);
    if offset < content_start || offset > content_end {
        return None;
    }
    let value = src[content_start..content_end].to_string();
    let parent = string.parent()?;
    // A string in a model's `$fillable` / `$casts` / … array.
    if let Some(model) = property_array_model(analyzer, parent, string) {
        return Some(Context {
            model,
            wanted: Wanted::Column,
            start: content_start,
            value,
            content_start,
        });
    }
    let (argument, element) = match parent.kind() {
        "argument" => (parent, None),
        "array_element_initializer" => {
            let array = parent
                .parent()
                .filter(|a| a.kind() == "array_creation_expression")?;
            (
                array.parent().filter(|a| a.kind() == "argument")?,
                Some(parent),
            )
        }
        _ => return None,
    };
    let call = argument.parent()?.parent()?;
    let method = text(call.child_by_field_name("name")?, src).to_ascii_lowercase();
    let first = argument
        .parent()
        .and_then(|args| {
            args.named_children(&mut args.walk())
                .find(|a| a.kind() == "argument")
        })
        .is_some_and(|a| a.id() == argument.id());
    if !first {
        return None;
    }
    let is_key = element.is_some_and(|e| {
        e.named_child(0).is_some_and(|k| k.id() == string.id()) && e.named_child(1).is_some()
    });
    let method = method.as_str();
    let wanted = match element {
        None if COLUMN_CALLS.contains(&method) => Wanted::Column,
        None if RELATION_CALLS.contains(&method) => Wanted::Relation,
        Some(_) if is_key && COLUMN_KEY_CALLS.contains(&method) => Wanted::Column,
        Some(_) if !is_key && COLUMN_LIST_CALLS.contains(&method) => Wanted::Column,
        Some(_)
            if RELATION_CALLS.contains(&method)
                && (is_key || element.is_some_and(|e| e.named_child(1).is_none())) =>
        {
            Wanted::Relation
        }
        _ => return None,
    };
    let model = receiver_model(analyzer, call)?;
    let start = match wanted {
        Wanted::Column => content_start,
        Wanted::Relation => {
            content_start
                + value[..offset - content_start]
                    .rfind('.')
                    .map_or(0, |i| i + 1)
        }
    };
    Some(Context {
        model,
        wanted,
        start,
        value,
        content_start,
    })
}

/// The string literal node containing `offset` (its quotes included).
fn string_at(root: Node, offset: usize) -> Option<Node> {
    let mut node = root.descendant_for_byte_range(offset, offset)?;
    loop {
        if matches!(node.kind(), "string" | "encapsed_string") {
            return Some(node);
        }
        node = node.parent()?;
    }
}

/// The model whose `$fillable`-like property array holds `string`.
fn property_array_model(analyzer: &Analyzer, parent: Node, string: Node) -> Option<String> {
    let element = (parent.kind() == "array_element_initializer").then_some(parent)?;
    let array = element
        .parent()
        .filter(|a| a.kind() == "array_creation_expression")?;
    let mut holder = array.parent()?;
    if holder.kind() == "property_initializer" {
        holder = holder.parent()?;
    }
    if holder.kind() != "property_element" {
        return None;
    }
    let name = analyzer.text(holder.named_child(0)?);
    if !COLUMN_PROPERTIES.contains(&name) {
        return None;
    }
    // `$casts` maps column => cast: only its keys are columns.
    let is_key = element
        .named_child(0)
        .is_some_and(|k| k.id() == string.id());
    if name == "$casts" && !is_key {
        return None;
    }
    let ctx = analyzer.ctx(string.start_byte());
    let class = analyzer.enclosing_class(string, &ctx)?;
    is_model(analyzer.index, &class).then_some(class)
}

/// The model a query call works on: its receiver is a model, a
/// `Builder<Model>` or a relation to a model, or `$this` in a model.
fn receiver_model(analyzer: &Analyzer, call: Node) -> Option<String> {
    let ty = match call.kind() {
        "scoped_call_expression" => {
            Ty::class(analyzer.class_of_scope(call.child_by_field_name("scope")?, 0)?)
        }
        _ => analyzer.ty_of(call.child_by_field_name("object")?, 0),
    };
    model_of(analyzer.index, &ty)
}

/// The model a type stands for: the class itself when a model, else a
/// model among its generic arguments or element type (builders, relations,
/// collections).
fn model_of(index: &Index, ty: &Ty) -> Option<String> {
    let class = ty.class.as_deref()?;
    if is_model(index, class) {
        return Some(class.to_string());
    }
    ty.args
        .iter()
        .chain(ty.elem.iter())
        .find(|a| !a.is_empty() && is_model(index, a))
        .cloned()
}

/// The model's migration columns, with the migration file.
fn columns(index: &Index, model: &str) -> Vec<(PathBuf, Symbol)> {
    let mut out: Vec<(PathBuf, Symbol)> = Vec::new();
    for table in index
        .hierarchy(model)
        .into_iter()
        .filter(|c| c.starts_with(TABLE_PREFIX))
    {
        for (path, sym) in index.members(&table) {
            if sym.kind == Kind::Property && !out.iter().any(|(_, s)| s.name == sym.name) {
                out.push((path.to_path_buf(), sym.clone()));
            }
        }
    }
    out
}

/// Detail shown for a column: its PHP type.
fn column_detail(sym: &Symbol) -> String {
    sym.ty
        .class
        .as_deref()
        .map(|c| c.rsplit('\\').next().unwrap_or(c).to_string())
        .unwrap_or_else(|| sym.signature.clone())
}

/// Relation methods of `model`, with the related model's short name when
/// known: methods returning an Eloquent relation, or those Eloquent
/// conventions turned into a property of the same name.
fn relations(index: &Index, model: &str) -> Vec<(Symbol, Option<String>)> {
    let members = index.members(model);
    members
        .iter()
        .filter(|(_, s)| s.kind == Kind::Method && !s.is_static)
        .filter(|(_, s)| is_relation(index, &members, s))
        .map(|(_, s)| {
            let related = related_of(index, &members, s);
            let short = related.map(|r| r.rsplit('\\').next().unwrap_or(&r).to_string());
            ((*s).clone(), short)
        })
        .collect()
}

/// Whether method `s` is a relation.
fn is_relation(index: &Index, members: &[(&std::path::Path, &Symbol)], s: &Symbol) -> bool {
    let returns_relation = s.ty.class.as_deref().is_some_and(|c| {
        c.starts_with(RELATIONS_NAMESPACE)
            || index
                .hierarchy(c)
                .iter()
                .any(|h| h.starts_with(RELATIONS_NAMESPACE))
    });
    returns_relation
        || members.iter().any(|(_, p)| {
            p.kind == Kind::Property
                && p.is_virtual
                && p.name == s.name
                && model_of(index, &p.ty).is_some()
        })
}

/// The model a relation method leads to, from its return type's generic
/// arguments or the property Eloquent derives from it.
fn related_of(
    index: &Index,
    members: &[(&std::path::Path, &Symbol)],
    s: &Symbol,
) -> Option<String> {
    let from_return =
        s.ty.args
            .iter()
            .find(|a| !a.is_empty() && is_model(index, a))
            .cloned();
    from_return.or_else(|| {
        members
            .iter()
            .find(|(_, p)| p.kind == Kind::Property && p.name == s.name)
            .and_then(|(_, p)| model_of(index, &p.ty))
    })
}

/// The relation method `name` of `model`.
fn relation_method(index: &Index, model: &str, name: &str) -> Option<(PathBuf, Symbol)> {
    let members = index.members(model);
    members
        .iter()
        .find(|(_, s)| s.kind == Kind::Method && s.name.eq_ignore_ascii_case(name))
        .map(|(p, s)| (p.to_path_buf(), (*s).clone()))
}

/// The model relation `name` of `model` leads to.
fn related_model(index: &Index, model: &str, name: &str) -> Option<String> {
    let members = index.members(model);
    let method = members
        .iter()
        .find(|(_, s)| s.kind == Kind::Method && s.name.eq_ignore_ascii_case(name))?;
    related_of(index, &members, method.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::index_source;

    /// Model `Post` with a `posts` table and an `author` relation to `User`.
    fn index() -> Index {
        let mut idx = Index::default();
        let files = [
            (
                "/vendor/Model.php",
                "<?php namespace Illuminate\\Database\\Eloquent; abstract class Model {} class Builder {}",
            ),
            (
                "/app/User.php",
                "<?php namespace App; use Illuminate\\Database\\Eloquent\\Model; class User extends Model {}",
            ),
            (
                "/app/Post.php",
                "<?php namespace App; use Illuminate\\Database\\Eloquent\\Model; class Post extends Model { protected $fillable = ['title']; public function author() { return $this->belongsTo(User::class); } }",
            ),
            (
                "/database/migrations/2024_create_posts.php",
                "<?php Schema::create('posts', function (Blueprint $table) { $table->id(); $table->string('title'); $table->text('body'); });",
            ),
        ];
        for (path, src) in files {
            idx.update(path.into(), index_source(src));
        }
        idx
    }

    fn labels(src: &str) -> Vec<String> {
        let offset = src.find('|').unwrap();
        let src = src.replacen('|', "", 1);
        eloquent_completions(&index(), &src, offset)
            .map(|(_, items)| items.into_iter().map(|c| c.label).collect())
            .unwrap_or_default()
    }

    #[test]
    fn eloquent_completions_should_offer_columns_in_where() {
        let got = labels("<?php use App\\Post; Post::where('ti|');");
        assert!(got.contains(&"title".to_string()) && got.contains(&"body".to_string()));
    }

    #[test]
    fn eloquent_completions_should_offer_columns_as_create_keys() {
        assert!(
            labels("<?php use App\\Post; Post::create(['|' => 1]);").contains(&"title".to_string())
        );
    }

    #[test]
    fn eloquent_completions_should_offer_relations_in_with() {
        assert_eq!(
            labels("<?php use App\\Post; Post::with('au|');"),
            ["author"]
        );
    }

    #[test]
    fn eloquent_completions_should_ignore_other_strings() {
        assert!(labels("<?php use App\\Post; Post::find('ti|');").is_empty());
    }
}
