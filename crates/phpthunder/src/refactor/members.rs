//! Refactorings that move or remove class members across files: safe
//! delete, make static, extract interface, pull members up and push them
//! down. Each returns a [`RenamePlan`] (edits per file, plus files to
//! create or delete) or the reason it cannot be done.

use super::*;
use crate::analysis::{Analyzer, Definition, Target, find_references, resolve};
use crate::index::Index;
use crate::inspection::{Edit, descendants, remove_lines_edit};
use crate::syntax::{context_at, import_insertion_point, is_class_like, parse, text};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Whether `path` is outside the project's own code (vendor packages).
fn is_vendor(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "vendor")
}

/// The declaration node a symbol's `name_start` belongs to.
fn declaration_at(root: Node, name_start: usize) -> Option<Node> {
    let mut n = root.descendant_for_byte_range(name_start, name_start)?;
    loop {
        if is_class_like(n.kind())
            || matches!(
                n.kind(),
                "method_declaration"
                    | "function_definition"
                    | "property_declaration"
                    | "const_declaration"
                    | "property_promotion_parameter"
                    | "enum_case"
            )
        {
            return Some(n);
        }
        n = n.parent()?;
    }
}

/// Start of `decl` including a doc comment right before it.
fn start_with_doc(decl: Node, src: &str) -> usize {
    decl.prev_named_sibling()
        .filter(|p| p.kind() == "comment" && text(*p, src).starts_with("/**"))
        .map_or(decl.start_byte(), |d| d.start_byte())
}

/// The definitions of the symbol at `byte`, refusing ones outside the
/// project.
fn definitions_here(index: &Index, src: &str, byte: usize) -> Result<Vec<Definition>, String> {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    let (target, _) = analyzer
        .target_at(byte)
        .ok_or("Place the caret on a class, function, method or constant")?;
    let defs = resolve(index, &target);
    let first = defs.first().ok_or("Cannot find the declaration")?;
    if defs.iter().any(|d| is_vendor(&d.path)) {
        return Err(format!(
            "'{}' is declared outside the project",
            first.symbol.name
        ));
    }
    Ok(defs)
}

/// Deletes the declaration at `byte` when nothing uses it; otherwise
/// reports where it is used. A class alone in its file deletes the file.
///
/// # Errors
/// Returns the usages found, or why the symbol cannot be deleted.
pub fn safe_delete(
    index: &Index,
    src: &str,
    byte: usize,
    load: impl Fn(&Path) -> Option<String>,
) -> Result<RenamePlan, String> {
    let defs = definitions_here(index, src, byte)?;
    let usages: Vec<_> = find_references(index, &defs, &load)
        .into_iter()
        .filter(|u| !u.is_declaration)
        .collect();
    let def = &defs[0];
    if let Some(first) = usages.first() {
        return Err(format!(
            "'{}' is used {} time(s), first in {}:{}",
            def.symbol.name,
            usages.len(),
            first.path.display(),
            first.span.start.line + 1
        ));
    }
    let file = load(&def.path).ok_or("Cannot read the declaring file")?;
    let tree = parse(&file);
    let root = tree.root_node();
    let decl = declaration_at(root, def.symbol.name_start).ok_or("Cannot find the declaration")?;
    let mut plan = RenamePlan::default();
    if is_class_like(decl.kind()) {
        let classes = descendants(root)
            .into_iter()
            .filter(|n| is_class_like(n.kind()))
            .count();
        if classes == 1 {
            plan.deleted.push(def.path.clone());
            return Ok(plan);
        }
    }
    let edit = match decl.kind() {
        "property_promotion_parameter" => {
            return Err(
                "A promoted property is a constructor parameter; change the signature instead"
                    .into(),
            );
        }
        "property_declaration" | "const_declaration" => {
            let mut c = decl.walk();
            let elements: Vec<Node> = decl
                .named_children(&mut c)
                .filter(|e| matches!(e.kind(), "property_element" | "const_element"))
                .collect();
            if elements.len() > 1 {
                return Err("Split the declaration first: it declares several members".into());
            }
            remove_lines_edit(&file, start_with_doc(decl, &file), decl.end_byte())
        }
        _ => remove_lines_edit(&file, start_with_doc(decl, &file), decl.end_byte()),
    };
    plan.edits.insert(
        def.path.clone(),
        vec![Edit {
            path: Some(def.path.clone()),
            ..edit
        }],
    );
    Ok(plan)
}

/// The method declaration whose header (or name) holds `byte`.
fn method_at(root: Node, byte: usize) -> Option<Node> {
    let mut n = root.descendant_for_byte_range(byte, byte)?;
    loop {
        if n.kind() == "method_declaration" {
            let in_header = n
                .child_by_field_name("body")
                .is_none_or(|b| byte <= b.start_byte());
            return in_header.then_some(n);
        }
        n = n.parent()?;
    }
}

/// Whether `byte` is in the header of a method declaration (where the
/// member refactorings apply).
pub fn method_header_at(src: &str, byte: usize) -> bool {
    method_at(parse(src).root_node(), byte).is_some()
}

/// Whether `byte` is in the header of a class declaration (before its
/// body), where extract interface applies.
pub fn class_header_at(src: &str, byte: usize) -> bool {
    let tree = parse(src);
    let mut cur = tree.root_node().descendant_for_byte_range(byte, byte);
    while let Some(n) = cur {
        if n.kind() == "class_declaration" {
            return n
                .child_by_field_name("body")
                .is_some_and(|b| byte < b.start_byte());
        }
        cur = n.parent();
    }
    false
}

/// The class declaration around `node` and its FQN.
fn class_of<'t>(node: Node<'t>, src: &str, root: Node<'t>) -> Option<(Node<'t>, String)> {
    let mut n = node.parent()?;
    while !is_class_like(n.kind()) {
        n = n.parent()?;
    }
    let name = text(n.child_by_field_name("name")?, src);
    Some((n, context_at(root, src, n.start_byte()).qualify(name)))
}

/// Makes the method at `byte` static: adds `static` and turns its calls
/// into static calls (`$this->m()` becomes `static::m()`, `$x->m()`
/// becomes `\Owner::m()`).
///
/// # Errors
/// Returns why the method cannot be static (it uses `$this`, is abstract).
pub fn make_static(
    index: &Index,
    path: &Path,
    src: &str,
    byte: usize,
    load: impl Fn(&Path) -> Option<String>,
) -> Result<RenamePlan, String> {
    let tree = parse(src);
    let root = tree.root_node();
    let method = method_at(root, byte).ok_or("Place the caret on a method declaration")?;
    let mut c = method.walk();
    let children: Vec<Node> = method.children(&mut c).collect();
    if children.iter().any(|n| n.kind() == "static_modifier") {
        return Err("The method is already static".into());
    }
    let body = method
        .child_by_field_name("body")
        .ok_or("An abstract method cannot be made static")?;
    if descendants(body)
        .iter()
        .any(|n| n.kind() == "variable_name" && text(*n, src) == "$this")
    {
        return Err("The method uses $this".into());
    }
    let (_, owner) = class_of(method, src, root).ok_or("The method is not in a named class")?;
    let keyword = children
        .iter()
        .find(|n| n.kind() == "function")
        .ok_or("Cannot find the function keyword")?;
    let mut plan = RenamePlan::default();
    let mut add = |file: &Path, edit: Edit| {
        plan.edits
            .entry(file.to_path_buf())
            .or_default()
            .push(Edit {
                path: Some(file.to_path_buf()),
                ..edit
            });
    };
    add(
        path,
        Edit {
            path: None,
            start: keyword.start_byte(),
            end: keyword.start_byte(),
            text: "static ".into(),
        },
    );
    let name = method.child_by_field_name("name").ok_or("Unnamed method")?;
    let analyzer = Analyzer::new(index, src, &tree);
    let defs = analyzer
        .target_of(name)
        .map(|t| resolve(index, &t))
        .unwrap_or_default();
    for usage in find_references(index, &defs, &load) {
        if usage.is_declaration {
            continue;
        }
        let Some(file) = load(&usage.path) else {
            continue;
        };
        let lines = crate::syntax::LineIndex::new(&file);
        let start = lines.offset(&file, usage.span.start);
        let file_tree = parse(&file);
        let Some(call) = file_tree
            .root_node()
            .descendant_for_byte_range(start, start)
            .and_then(|n| n.parent())
            .filter(|p| {
                matches!(
                    p.kind(),
                    "member_call_expression" | "nullsafe_member_call_expression"
                )
            })
        else {
            continue;
        };
        let Some(object) = call.child_by_field_name("object") else {
            continue;
        };
        let replacement = if text(object, &file) == "$this" {
            "static::".to_string()
        } else {
            format!("\\{owner}::")
        };
        add(
            &usage.path,
            Edit {
                path: None,
                start: object.start_byte(),
                end: start,
                text: replacement,
            },
        );
    }
    for edits in plan.edits.values_mut() {
        edits.sort_by_key(|e| e.start);
    }
    Ok(plan)
}

/// Whether `name` is a valid PHP identifier.
fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Creates interface `name` next to the class at `byte`, declaring its
/// public methods, and makes the class implement it.
///
/// # Errors
/// Returns why the interface cannot be extracted (bad name, file exists,
/// no public methods).
pub fn extract_interface(
    path: &Path,
    src: &str,
    byte: usize,
    name: &str,
    load: impl Fn(&Path) -> Option<String>,
) -> Result<RenamePlan, String> {
    let name = name.trim();
    if !valid_identifier(name) {
        return Err(format!("'{name}' is not a valid name"));
    }
    let new_path = path.with_file_name(format!("{name}.php"));
    if load(&new_path).is_some() {
        return Err(format!("{} already exists", new_path.display()));
    }
    let tree = parse(src);
    let root = tree.root_node();
    let mut class = root
        .descendant_for_byte_range(byte, byte)
        .ok_or("Place the caret in a class")?;
    while class.kind() != "class_declaration" {
        class = class.parent().ok_or("Place the caret in a class")?;
    }
    let body = class
        .child_by_field_name("body")
        .ok_or("The class has no body")?;
    let mut bc = body.walk();
    let mut methods = Vec::new();
    for m in body
        .named_children(&mut bc)
        .filter(|m| m.kind() == "method_declaration")
    {
        let mut mc = m.walk();
        let public = !m
            .children(&mut mc)
            .any(|n| n.kind() == "visibility_modifier" && text(n, src) != "public");
        let constructor = m
            .child_by_field_name("name")
            .is_some_and(|n| text(n, src).starts_with("__"));
        if !public || constructor {
            continue;
        }
        let header_end = m
            .child_by_field_name("body")
            .map_or(m.end_byte(), |b| b.start_byte());
        let header: String = src[m.start_byte()..header_end]
            .split_whitespace()
            .filter(|w| !matches!(*w, "final" | "abstract"))
            .collect::<Vec<_>>()
            .join(" ");
        let header = header.trim_end_matches(';');
        let doc = m
            .prev_named_sibling()
            .filter(|d| d.kind() == "comment" && text(*d, src).starts_with("/**"))
            .map(|d| format!("    {}\n", text(d, src).trim()))
            .unwrap_or_default();
        methods.push(format!("{doc}    {header};\n"));
    }
    if methods.is_empty() {
        return Err("The class has no public methods".into());
    }
    let ctx = context_at(root, src, class.start_byte());
    let mut content = String::from("<?php\n\n");
    if src.contains("strict_types=1") {
        content.push_str("declare(strict_types=1);\n\n");
    }
    if !ctx.namespace.is_empty() {
        content.push_str(&format!("namespace {};\n\n", ctx.namespace));
    }
    let uses: Vec<&str> = descendants(root)
        .into_iter()
        .filter(|n| n.kind() == "namespace_use_declaration")
        .map(|n| text(n, src))
        .collect();
    if !uses.is_empty() {
        content.push_str(&uses.join("\n"));
        content.push_str("\n\n");
    }
    content.push_str(&format!("interface {name}\n{{\n{}}}\n", methods.join("\n")));
    let mut c = class.walk();
    let clauses: Vec<Node> = class.named_children(&mut c).collect();
    let edit = match clauses
        .iter()
        .find(|n| n.kind() == "class_interface_clause")
    {
        Some(implements) => Edit {
            path: Some(path.to_path_buf()),
            start: implements.end_byte(),
            end: implements.end_byte(),
            text: format!(", {name}"),
        },
        None => {
            let after = clauses
                .iter()
                .find(|n| n.kind() == "base_clause")
                .copied()
                .or_else(|| class.child_by_field_name("name"))
                .ok_or("The class has no name")?;
            Edit {
                path: Some(path.to_path_buf()),
                start: after.end_byte(),
                end: after.end_byte(),
                text: format!(" implements {name}"),
            }
        }
    };
    let mut plan = RenamePlan::default();
    plan.edits.insert(path.to_path_buf(), vec![edit]);
    plan.created.push((new_path, content));
    Ok(plan)
}

/// `use` statements `to_src` needs for the classes `node` (from
/// `from_src`) names by short name.
fn needed_imports(
    index: &Index,
    from_src: &str,
    from_tree: &tree_sitter::Tree,
    node: Node,
    to_src: &str,
    to_root: Node,
    at: usize,
) -> Vec<String> {
    let analyzer = Analyzer::new(index, from_src, from_tree);
    let target_ctx = context_at(to_root, to_src, at);
    let mut out = BTreeSet::new();
    for n in descendants(node) {
        if n.kind() != "name"
            || n.parent()
                .is_some_and(|p| matches!(p.kind(), "qualified_name" | "namespace_name"))
        {
            continue;
        }
        let written = text(n, from_src);
        if matches!(
            written.to_ascii_lowercase().as_str(),
            "self" | "static" | "parent"
        ) {
            continue;
        }
        if let Some(Target::Class(fqn)) = analyzer.target_of(n)
            && !target_ctx.resolve_class(written).eq_ignore_ascii_case(&fqn)
            && !target_ctx
                .classes
                .contains_key(&written.to_ascii_lowercase())
        {
            out.insert(format!("\nuse {fqn};"));
        }
    }
    out.into_iter().collect()
}

/// The method at `byte` with its doc comment, its class and the class's
/// private member names.
struct MovedMethod<'t> {
    /// The method declaration.
    method: Node<'t>,
    /// The class declaring it.
    class: Node<'t>,
    /// The class's FQN.
    owner: String,
    /// Method name.
    name: String,
    /// Source from the start of its line (doc comment included) to its end.
    text: String,
}

/// Finds the method to move at `byte`.
fn moved_method<'t>(root: Node<'t>, src: &str, byte: usize) -> Result<MovedMethod<'t>, String> {
    let method = method_at(root, byte).ok_or("Place the caret on a method declaration")?;
    let (class, owner) = class_of(method, src, root).ok_or("The method is not in a named class")?;
    let name = text(
        method.child_by_field_name("name").ok_or("Unnamed method")?,
        src,
    )
    .to_string();
    let start = start_with_doc(method, src);
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    Ok(MovedMethod {
        method,
        class,
        owner,
        name,
        text: src[line_start..method.end_byte()].to_string(),
    })
}

/// Names of private methods and properties declared in `class`.
fn private_members(class: Node, src: &str) -> Vec<String> {
    let Some(body) = class.child_by_field_name("body") else {
        return Vec::new();
    };
    let mut c = body.walk();
    body.named_children(&mut c)
        .filter(|m| {
            let mut mc = m.walk();
            m.children(&mut mc)
                .any(|n| n.kind() == "visibility_modifier" && text(n, src) == "private")
        })
        .flat_map(|m| {
            descendants(m)
                .into_iter()
                .filter(|n| {
                    n.parent().is_some_and(|p| {
                        matches!(p.kind(), "method_declaration" | "property_element")
                    }) && matches!(n.kind(), "name" | "variable_name")
                })
                .map(|n| text(n, src).trim_start_matches('$').to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A method being copied into another class: where it comes from and
/// the text to insert.
struct MethodCopy<'a> {
    /// Project index (to resolve the class names it uses).
    index: &'a Index,
    /// Source of the file it comes from.
    src: &'a str,
    /// Syntax tree of `src`.
    tree: &'a tree_sitter::Tree,
    /// The method declaration in `tree`.
    method: Node<'a>,
    /// Text to insert (possibly with changed visibility).
    text: &'a str,
}

/// Inserts the copied method (and the imports it needs) at the end of
/// the class declared at `name_start` in `file`.
fn insertion(
    copy: &MethodCopy,
    file: &str,
    file_path: &Path,
    name_start: usize,
) -> Result<Vec<Edit>, String> {
    let tree = parse(file);
    let root = tree.root_node();
    let class = declaration_at(root, name_start).ok_or("Cannot find the target class")?;
    let body = class
        .child_by_field_name("body")
        .ok_or("The target class has no body")?;
    let close = body.end_byte() - 1;
    let mut edits = vec![Edit {
        path: Some(file_path.to_path_buf()),
        start: close,
        end: close,
        text: format!("\n{}\n", copy.text.trim_end()),
    }];
    let at = import_insertion_point(root, class.start_byte());
    for import in needed_imports(
        copy.index,
        copy.src,
        copy.tree,
        copy.method,
        file,
        root,
        class.start_byte(),
    ) {
        edits.push(Edit {
            path: Some(file_path.to_path_buf()),
            start: at,
            end: at,
            text: import,
        });
    }
    Ok(edits)
}

/// Moves the method at `byte` to the parent class (private becomes
/// protected).
///
/// # Errors
/// Returns why the method cannot move (no project parent, parent declares
/// it, it uses private members of its class).
pub fn pull_up(
    index: &Index,
    path: &Path,
    src: &str,
    byte: usize,
    load: impl Fn(&Path) -> Option<String>,
) -> Result<RenamePlan, String> {
    let tree = parse(src);
    let root = tree.root_node();
    let moved = moved_method(root, src, byte)?;
    let parent = index
        .classes(&moved.owner)
        .into_iter()
        .find_map(|(_, s)| s.extends.clone())
        .ok_or("The class has no parent class")?;
    let (parent_path, parent_sym) = index
        .classes(&parent)
        .into_iter()
        .next()
        .map(|(p, s)| (p.to_path_buf(), s.clone()))
        .ok_or("The parent class is not indexed")?;
    if is_vendor(&parent_path) {
        return Err("The parent class is outside the project".into());
    }
    if !index
        .lookup(&crate::index::member_key(
            &parent,
            crate::symbol::MemberTag::Method,
            &moved.name,
        ))
        .is_empty()
    {
        return Err(format!(
            "{} already declares {}()",
            parent_sym.name, moved.name
        ));
    }
    let privates = private_members(moved.class, src);
    let body = text(moved.method, src);
    if let Some(p) = privates.iter().find(|p| {
        **p != moved.name
            && (body.contains(&format!("->{p}"))
                || body.contains(&format!("::{p}"))
                || body.contains(&format!("::${p}")))
    }) {
        return Err(format!("The method uses private member {p} of its class"));
    }
    let method_text = moved
        .text
        .replacen("private function", "protected function", 1);
    let parent_src = load(&parent_path).ok_or("Cannot read the parent class")?;
    let mut plan = RenamePlan::default();
    plan.edits.insert(
        parent_path.clone(),
        insertion(
            &MethodCopy {
                index,
                src,
                tree: &tree,
                method: moved.method,
                text: &method_text,
            },
            &parent_src,
            &parent_path,
            parent_sym.name_start,
        )?,
    );
    let start = start_with_doc(moved.method, src);
    plan.edits.insert(
        path.to_path_buf(),
        vec![Edit {
            path: Some(path.to_path_buf()),
            ..remove_lines_edit(src, start, moved.method.end_byte())
        }],
    );
    Ok(plan)
}

/// Moves the method at `byte` into every direct subclass that does not
/// declare it, and removes it from its class.
///
/// # Errors
/// Returns why the method cannot move (no subclasses in the project, the
/// class itself calls it).
pub fn push_down(
    index: &Index,
    path: &Path,
    src: &str,
    byte: usize,
    load: impl Fn(&Path) -> Option<String>,
) -> Result<RenamePlan, String> {
    let tree = parse(src);
    let root = tree.root_node();
    let moved = moved_method(root, src, byte)?;
    let subclasses: Vec<(PathBuf, crate::symbol::Symbol)> = subtypes(index, &moved.owner)
        .into_iter()
        .filter(|(_, s)| {
            s.extends
                .as_deref()
                .is_some_and(|e| e.eq_ignore_ascii_case(&moved.owner))
        })
        .collect();
    if subclasses.is_empty() {
        return Err("The class has no subclasses".into());
    }
    if let Some((p, _)) = subclasses.iter().find(|(p, _)| is_vendor(p)) {
        return Err(format!(
            "A subclass is outside the project: {}",
            p.display()
        ));
    }
    let class_text = text(moved.class, src).replace(text(moved.method, src), "");
    let lower = class_text.to_ascii_lowercase();
    let name = moved.name.to_ascii_lowercase();
    if [
        format!("->{name}("),
        format!("::{name}("),
        format!("'{name}'"),
    ]
    .iter()
    .any(|p| lower.contains(p.as_str()))
    {
        return Err(format!("{}() is used by its own class", moved.name));
    }
    let mut plan = RenamePlan::default();
    for (sub_path, sub) in &subclasses {
        let declares = !index
            .lookup(&crate::index::member_key(
                &sub.fqn,
                crate::symbol::MemberTag::Method,
                &moved.name,
            ))
            .is_empty();
        if declares {
            continue;
        }
        let sub_src = load(sub_path).ok_or("Cannot read a subclass")?;
        let edits = insertion(
            &MethodCopy {
                index,
                src,
                tree: &tree,
                method: moved.method,
                text: &moved.text,
            },
            &sub_src,
            sub_path,
            sub.name_start,
        )?;
        plan.edits
            .entry(sub_path.clone())
            .or_default()
            .extend(edits);
    }
    let start = start_with_doc(moved.method, src);
    plan.edits
        .entry(path.to_path_buf())
        .or_default()
        .push(Edit {
            path: Some(path.to_path_buf()),
            ..remove_lines_edit(src, start, moved.method.end_byte())
        });
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::index_source;
    use std::collections::HashMap;

    /// An index and a loader over in-memory files.
    fn project(files: &[(&str, &str)]) -> (Index, HashMap<PathBuf, String>) {
        let mut index = Index::default();
        let mut map = HashMap::new();
        for (p, s) in files {
            index.update(PathBuf::from(p), index_source(s));
            map.insert(PathBuf::from(p), s.to_string());
        }
        (index, map)
    }

    /// `files` after applying `plan`.
    fn applied(files: &HashMap<PathBuf, String>, plan: &RenamePlan) -> HashMap<PathBuf, String> {
        let mut out = files.clone();
        for (p, edits) in &plan.edits {
            let text = out.get(p).cloned().unwrap_or_default();
            out.insert(p.clone(), apply_edits(&text, edits));
        }
        for (p, content) in &plan.created {
            out.insert(p.clone(), content.clone());
        }
        for p in &plan.deleted {
            out.remove(p);
        }
        out
    }

    #[test]
    fn unused_method_should_be_deleted() {
        let a = "<?php\nclass A\n{\n    /** Doc. */\n    public function gone() {}\n\n    public function kept() {}\n}\n";
        let (index, files) = project(&[("/a.php", a)]);
        let plan = safe_delete(&index, a, a.find("gone").unwrap(), |p| {
            files.get(p).cloned()
        })
        .unwrap();
        assert_eq!(
            applied(&files, &plan)[Path::new("/a.php")],
            "<?php\nclass A\n{\n\n    public function kept() {}\n}\n"
        );
    }

    #[test]
    fn used_method_should_not_be_deleted() {
        let a =
            "<?php class A { public function used() {} public function f() { $this->used(); } }";
        let (index, files) = project(&[("/a.php", a)]);
        let err = safe_delete(&index, a, a.find("used()").unwrap(), |p| {
            files.get(p).cloned()
        })
        .unwrap_err();
        assert!(err.contains("used 1 time"));
    }

    #[test]
    fn unused_class_should_delete_its_file() {
        let a = "<?php\nclass Lonely {}\n";
        let (index, files) = project(&[("/Lonely.php", a)]);
        let plan = safe_delete(&index, a, a.find("Lonely").unwrap(), |p| {
            files.get(p).cloned()
        })
        .unwrap();
        assert_eq!(plan.deleted, [PathBuf::from("/Lonely.php")]);
    }

    #[test]
    fn method_should_become_static_with_its_calls() {
        let a = "<?php class A { public function twice($x) { return $x * 2; } public function f() { return $this->twice(1); } }";
        let b = "<?php function g(A $a) { return $a->twice(2); }";
        let (index, files) = project(&[("/a.php", a), ("/b.php", b)]);
        let plan = make_static(
            &index,
            Path::new("/a.php"),
            a,
            a.find("twice").unwrap(),
            |p| files.get(p).cloned(),
        )
        .unwrap();
        let out = applied(&files, &plan);
        assert_eq!(
            out[Path::new("/a.php")],
            "<?php class A { public static function twice($x) { return $x * 2; } public function f() { return static::twice(1); } }"
        );
        assert_eq!(
            out[Path::new("/b.php")],
            "<?php function g(A $a) { return \\A::twice(2); }"
        );
    }

    #[test]
    fn method_using_this_should_not_become_static() {
        let a = "<?php class A { public function f() { return $this; } }";
        let (index, files) = project(&[("/a.php", a)]);
        assert!(
            make_static(&index, Path::new("/a.php"), a, a.find("f(").unwrap(), |p| {
                files.get(p).cloned()
            })
            .is_err()
        );
    }

    #[test]
    fn interface_should_be_extracted_from_public_methods() {
        let a = "<?php\n\nnamespace App;\n\nuse Lib\\User;\n\nclass Repo extends Base\n{\n    public function find(int $id): ?User { return null; }\n    private function hidden() {}\n}\n";
        let plan = extract_interface(
            Path::new("/app/Repo.php"),
            a,
            a.find("Repo").unwrap(),
            "RepoContract",
            |_| None,
        )
        .unwrap();
        assert_eq!(
            plan.created,
            [(
                PathBuf::from("/app/RepoContract.php"),
                "<?php\n\nnamespace App;\n\nuse Lib\\User;\n\ninterface RepoContract\n{\n    public function find(int $id): ?User;\n}\n".to_string()
            )]
        );
        let edits = &plan.edits[Path::new("/app/Repo.php")];
        assert_eq!(
            apply_edits(a, edits).lines().nth(6),
            Some("class Repo extends Base implements RepoContract")
        );
    }

    #[test]
    fn method_should_be_pulled_up_with_its_imports() {
        let base = "<?php\nnamespace App;\n\nclass Base\n{\n}\n";
        let child = "<?php\nnamespace App;\n\nuse Lib\\Mailer;\n\nclass Child extends Base\n{\n    private function send(Mailer $m) {}\n}\n";
        let (index, files) = project(&[("/Base.php", base), ("/Child.php", child)]);
        let plan = pull_up(
            &index,
            Path::new("/Child.php"),
            child,
            child.find("send").unwrap(),
            |p| files.get(p).cloned(),
        )
        .unwrap();
        let out = applied(&files, &plan);
        assert_eq!(
            out[Path::new("/Base.php")],
            "<?php\nnamespace App;\nuse Lib\\Mailer;\n\nclass Base\n{\n\n    protected function send(Mailer $m) {}\n}\n"
        );
        assert_eq!(
            out[Path::new("/Child.php")],
            "<?php\nnamespace App;\n\nuse Lib\\Mailer;\n\nclass Child extends Base\n{\n}\n"
        );
    }

    #[test]
    fn method_should_be_pushed_down_to_subclasses() {
        let base = "<?php\nclass Base\n{\n    public function hook() {}\n}\n";
        let a = "<?php\nclass A extends Base\n{\n}\n";
        let b = "<?php\nclass B extends Base\n{\n    public function hook() { return 1; }\n}\n";
        let (index, files) = project(&[("/Base.php", base), ("/A.php", a), ("/B.php", b)]);
        let plan = push_down(
            &index,
            Path::new("/Base.php"),
            base,
            base.find("hook").unwrap(),
            |p| files.get(p).cloned(),
        )
        .unwrap();
        let out = applied(&files, &plan);
        assert_eq!(out[Path::new("/Base.php")], "<?php\nclass Base\n{\n}\n");
        assert_eq!(
            out[Path::new("/A.php")],
            "<?php\nclass A extends Base\n{\n\n    public function hook() {}\n}\n"
        );
        assert_eq!(out[Path::new("/B.php")], b);
    }
}
