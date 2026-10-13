//! Rename across the project, or of a local variable within its function.

use super::*;
use crate::analysis::{Analyzer, find_references, resolve};
use crate::index::Index;
use crate::inspection::Edit;
use crate::syntax::{parse, text};
use std::path::Path;

/// Whether `name` is a valid PHP identifier for the renamed symbol kind.
fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
}

/// Plans renaming the symbol at `byte` in `path` to `new_name`.
///
/// Local variables are renamed inside their function only; other symbols
/// in every file that references them.
///
/// # Errors
/// Returns a message when the caret is not on a renameable symbol, the
/// symbol is declared outside the project (`vendor/`, built-ins), or
/// `new_name` is not a valid identifier.
pub fn rename(
    index: &Index,
    path: &Path,
    src: &str,
    byte: usize,
    new_name: &str,
    load: impl Fn(&Path) -> Option<String>,
) -> Result<RenamePlan, String> {
    let new_name = new_name.trim().trim_start_matches('$');
    if !valid_identifier(new_name) {
        return Err(format!("'{new_name}' is not a valid name"));
    }
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    if let Some(plan) = rename_local_variable(&tree, src, byte, new_name, path) {
        return Ok(plan);
    }
    let (target, _) = analyzer
        .target_at(byte)
        .ok_or("Place the caret on a class, function, method, property or variable")?;
    let defs = resolve(index, &target);
    let first = defs.first().ok_or("Cannot find the declaration")?;
    if defs.iter().any(|d| {
        d.path.components().any(|c| c.as_os_str() == "vendor")
            || d.path.starts_with(std::env::temp_dir())
    }) {
        return Err(format!(
            "'{}' is declared outside the project and cannot be renamed",
            first.symbol.name
        ));
    }
    let usages = find_references(index, &defs, &load);
    let mut plan = RenamePlan::default();
    for usage in usages {
        let Some(text) = load(&usage.path) else {
            continue;
        };
        let lines = crate::syntax::LineIndex::new(&text);
        let start = lines.offset(&text, usage.span.start);
        let end = lines.offset(&text, usage.span.end);
        plan.edits
            .entry(usage.path.clone())
            .or_default()
            .push(Edit {
                path: Some(usage.path),
                start,
                end,
                text: new_name.to_string(),
            });
    }
    for edits in plan.edits.values_mut() {
        edits.sort_by_key(|e| e.start);
        edits.dedup_by_key(|e| e.start);
    }
    let symbol = &first.symbol;
    if symbol.kind.is_class_like()
        && first
            .path
            .file_stem()
            .is_some_and(|s| s.eq_ignore_ascii_case(&symbol.name))
    {
        let renamed = first.path.with_file_name(format!("{new_name}.php"));
        plan.file_rename = Some((first.path.clone(), renamed));
    }
    Ok(plan)
}

/// Renames a `$variable` within its enclosing function, or returns `None`
/// when the caret is not on a local variable.
fn rename_local_variable(
    tree: &tree_sitter::Tree,
    src: &str,
    byte: usize,
    new_name: &str,
    path: &Path,
) -> Option<RenamePlan> {
    let node = tree
        .root_node()
        .named_descendant_for_byte_range(byte, byte)?;
    let var = if node.kind() == "variable_name" {
        node
    } else {
        node.parent().filter(|p| p.kind() == "variable_name")?
    };
    let name = text(var, src);
    // Properties (`$this->x`, declarations) go through project-wide rename.
    let parent = var.parent()?;
    if name == "$this"
        || matches!(
            parent.kind(),
            "property_element"
                | "property_promotion_parameter"
                | "scoped_property_access_expression"
        )
    {
        return None;
    }
    let scope = {
        let mut cur = var.parent();
        loop {
            match cur {
                Some(n)
                    if matches!(
                        n.kind(),
                        "function_definition"
                            | "method_declaration"
                            | "anonymous_function"
                            | "arrow_function"
                    ) =>
                {
                    break n;
                }
                Some(n) => cur = n.parent(),
                None => break tree.root_node(),
            }
        }
    };
    let mut edits = Vec::new();
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            if n.id() == scope.id()
                || !matches!(
                    child.kind(),
                    "function_definition" | "method_declaration" | "class_declaration"
                )
            {
                stack.push(child);
            }
        }
        if n.kind() == "variable_name"
            && text(n, src) == name
            && let Some(ident) = n.named_child(0)
        {
            edits.push(Edit {
                path: Some(path.to_path_buf()),
                start: ident.start_byte(),
                end: ident.end_byte(),
                text: new_name.to_string(),
            });
        }
    }
    edits.sort_by_key(|e| e.start);
    let mut plan = RenamePlan::default();
    plan.edits.insert(path.to_path_buf(), edits);
    Some(plan)
}

/// Applies edits (sorted or not) to `text`, last first so offsets stay valid.
pub fn apply_edits(text: &str, edits: &[Edit]) -> String {
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    sorted.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = text.to_string();
    for e in sorted {
        if e.start <= e.end
            && e.end <= out.len()
            && out.is_char_boundary(e.start)
            && out.is_char_boundary(e.end)
        {
            out.replace_range(e.start..e.end, &e.text);
        }
    }
    out
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

    #[test]
    fn signature_help_should_track_the_active_parameter() {
        let index = index_with(&[(
            "/f.php",
            "<?php function pair(int $a, array $b = [1, 2], string ...$rest): void {}",
        )]);
        let src = "<?php pair(1, [3, 4], 'x', 'y');";
        let at = |needle: &str| src.find(needle).unwrap();
        let help = signature_help(&index, src, at("1,")).unwrap();
        assert_eq!(
            help.params,
            vec!["int $a", "array $b = [1, 2]", "string ...$rest"]
        );
        assert_eq!(help.active, 0);
        assert_eq!(signature_help(&index, src, at("[3")).unwrap().active, 1);
        assert_eq!(signature_help(&index, src, at("'y'")).unwrap().active, 2);
    }

    #[test]
    fn signature_help_should_resolve_constructors_and_methods() {
        let index = index_with(&[(
            "/u.php",
            "<?php class User { function __construct(string $name) {} function rename(string $to) {} }",
        )]);
        let src = "<?php $u = new User('a'); $u->rename('b');";
        let ctor = signature_help(&index, src, src.find("'a'").unwrap()).unwrap();
        assert_eq!(ctor.params, vec!["string $name"]);
        let method = signature_help(&index, src, src.find("'b'").unwrap()).unwrap();
        assert_eq!(method.params, vec!["string $to"]);
    }

    #[test]
    fn rename_should_update_declaration_and_usages_across_files() {
        let a = "<?php class Repo { function find() {} }";
        let b = "<?php function f(Repo $r) { $r->find(); }";
        let index = index_with(&[("/Repo.php", a), ("/b.php", b)]);
        let load = |p: &Path| match p.to_str() {
            Some("/Repo.php") => Some(a.to_string()),
            Some("/b.php") => Some(b.to_string()),
            _ => None,
        };
        let plan = rename(
            &index,
            Path::new("/b.php"),
            b,
            b.find("find").unwrap(),
            "lookup",
            load,
        )
        .unwrap();
        assert_eq!(
            apply_edits(a, &plan.edits[Path::new("/Repo.php")]),
            "<?php class Repo { function lookup() {} }"
        );
        assert_eq!(
            apply_edits(b, &plan.edits[Path::new("/b.php")]),
            "<?php function f(Repo $r) { $r->lookup(); }"
        );
        let class_plan = rename(
            &index,
            Path::new("/b.php"),
            b,
            b.find("Repo").unwrap(),
            "Store",
            load,
        )
        .unwrap();
        assert_eq!(
            class_plan.file_rename,
            Some((PathBuf::from("/Repo.php"), PathBuf::from("/Store.php")))
        );
    }

    #[test]
    fn rename_should_keep_local_variables_inside_their_function() {
        let src = "<?php function a() { $x = 1; return $x; } function b() { $x = 2; }";
        let index = Index::default();
        let plan = rename(
            &index,
            Path::new("/a.php"),
            src,
            src.find("$x").unwrap() + 1,
            "count",
            |_| None,
        )
        .unwrap();
        let out = apply_edits(src, &plan.edits[Path::new("/a.php")]);
        assert_eq!(
            out,
            "<?php function a() { $count = 1; return $count; } function b() { $x = 2; }"
        );
    }

    #[test]
    fn rename_should_reject_invalid_names() {
        let src = "<?php function a() { $x = 1; }";
        assert!(
            rename(
                &Index::default(),
                Path::new("/a.php"),
                src,
                22,
                "1bad",
                |_| None
            )
            .is_err()
        );
    }

    #[test]
    fn optimize_imports_should_remove_unused_and_sort() {
        let index = Index::default();
        let src = "<?php\nnamespace App;\nuse Zed\\Last;\nuse function Lib\\helper;\nuse Alpha\\First;\nuse Gone\\Unused;\nnew Last(); new First(); helper();\n";
        assert_eq!(
            optimize_imports(&index, src),
            "<?php\nnamespace App;\nuse Alpha\\First;\nuse Zed\\Last;\nuse function Lib\\helper;\nnew Last(); new First(); helper();\n"
        );
    }

    #[test]
    fn implementations_should_list_unimplemented_interface_methods() {
        let index = index_with(&[(
            "/a.php",
            "<?php interface Shape { function area(): float; function name(): string; } abstract class Base implements Shape { function name(): string { return ''; } abstract protected function sides(): int; } class Square extends Base {}",
        )]);
        let names: Vec<String> = missing_implementations(&index, "Square")
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, vec!["sides", "area"]);
        let subs: Vec<String> = subtypes(&index, "Shape")
            .into_iter()
            .map(|(_, s)| s.fqn)
            .collect();
        assert_eq!(subs, vec!["Base", "Square"]);
    }
}
