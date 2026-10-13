//! Project-wide refactorings that rewrite a declaration and its uses:
//! changing a function's or method's parameters, and moving a class to
//! another namespace.

use super::*;
use crate::analysis::{Analyzer, Definition, find_references, resolve};
use crate::index::Index;
use crate::inspection::Edit;
use crate::symbol::Kind;
use crate::syntax::{LineIndex, context_at, import_insertion_point, is_class_like, parse, text};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// One parameter of the new signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamChange {
    /// Position of this parameter in the old signature; `None` for a new one.
    pub from: Option<usize>,
    /// The parameter as declared (`int $count = 0`).
    pub declaration: String,
    /// What existing calls pass for a new parameter (`0`, `null`, …).
    pub default_argument: String,
}

/// Parameter declarations of a `formal_parameters` node.
fn parameter_nodes(params: Node) -> Vec<Node> {
    let mut c = params.walk();
    params
        .named_children(&mut c)
        .filter(|n| {
            matches!(
                n.kind(),
                "simple_parameter" | "variadic_parameter" | "property_promotion_parameter"
            )
        })
        .collect()
}

/// The call around the name at `byte` (function, method, static or `new`).
fn call_at(root: Node, byte: usize) -> Option<Node> {
    let mut node = root.descendant_for_byte_range(byte, byte)?;
    loop {
        if matches!(
            node.kind(),
            "function_call_expression"
                | "member_call_expression"
                | "nullsafe_member_call_expression"
                | "scoped_call_expression"
                | "object_creation_expression"
        ) {
            return Some(node);
        }
        node = node.parent()?;
    }
}

/// The declaration resolved at `byte`, which must be a function or method.
fn callable_at(index: &Index, src: &str, byte: usize) -> Result<Vec<Definition>, String> {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    let (target, _) = analyzer
        .target_at(byte)
        .ok_or("Place the caret on a function or method")?;
    let defs = resolve(index, &target);
    match defs.first() {
        Some(d) if matches!(d.symbol.kind, Kind::Function | Kind::Method) => Ok(defs),
        Some(_) => Err("Only functions and methods have parameters to change".into()),
        None => Err("Cannot find the declaration".into()),
    }
}

/// Plans changing the parameters of the function or method at `byte` to
/// `params`: the declaration gets the new list, and every call passes its
/// old arguments in the new order, with each new parameter's
/// [`ParamChange::default_argument`]. Calls missing a trailing optional
/// argument keep it missing.
///
/// # Errors
/// Returns a message when the caret is not on a function or method, or
/// its declaration is outside the project.
pub fn change_signature(
    index: &Index,
    src: &str,
    byte: usize,
    params: &[ParamChange],
    load: impl Fn(&Path) -> Option<String>,
) -> Result<RenamePlan, String> {
    let defs = callable_at(index, src, byte)?;
    let decl = &defs[0];
    if decl.path.components().any(|c| c.as_os_str() == "vendor") {
        return Err("The declaration is outside the project".into());
    }
    let mut plan = RenamePlan::default();
    let decl_text = load(&decl.path).ok_or("Cannot read the declaration")?;
    let decl_tree = parse(&decl_text);
    let name_node = decl_tree
        .root_node()
        .descendant_for_byte_range(decl.symbol.name_start, decl.symbol.name_start)
        .ok_or("Cannot find the declaration")?;
    let callable = name_node.parent().ok_or("Cannot find the declaration")?;
    let formal = callable
        .child_by_field_name("parameters")
        .ok_or("Cannot find the parameter list")?;
    let new_list = params
        .iter()
        .map(|p| p.declaration.trim().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    plan.edits.entry(decl.path.clone()).or_default().push(Edit {
        path: Some(decl.path.clone()),
        start: formal.start_byte(),
        end: formal.end_byte(),
        text: format!("({new_list})"),
    });
    let old_count = parameter_nodes(formal).len();
    if params
        .iter()
        .any(|p| p.from.is_some_and(|i| i >= old_count))
    {
        return Err("A parameter refers to a position the old signature does not have".into());
    }
    for usage in find_references(index, &defs, &load)
        .into_iter()
        .filter(|u| !u.is_declaration)
    {
        let Some(text_now) = load(&usage.path) else {
            continue;
        };
        let lines = LineIndex::new(&text_now);
        let at = lines.offset(&text_now, usage.span.start);
        let tree = parse(&text_now);
        let Some(args) =
            call_at(tree.root_node(), at).and_then(|c| c.child_by_field_name("arguments"))
        else {
            continue;
        };
        let mut c = args.walk();
        let old_args: Vec<&str> = args
            .named_children(&mut c)
            .filter(|a| a.kind() == "argument")
            .map(|a| text(a, &text_now))
            .collect();
        let mut new_args: Vec<String> = Vec::new();
        for p in params {
            match p.from {
                Some(i) => match old_args.get(i) {
                    Some(arg) => new_args.push((*arg).to_string()),
                    None => break,
                },
                None => new_args.push(p.default_argument.clone()),
            }
        }
        plan.edits
            .entry(usage.path.clone())
            .or_default()
            .push(Edit {
                path: Some(usage.path.clone()),
                start: args.start_byte(),
                end: args.end_byte(),
                text: format!("({})", new_args.join(", ")),
            });
    }
    for edits in plan.edits.values_mut() {
        edits.sort_by_key(|e| e.start);
        edits.dedup_by_key(|e| e.start);
    }
    Ok(plan)
}

/// Plans moving the class declared in `path` (text `src`) to
/// `new_namespace`, optionally renaming its file to `new_path`:
/// - the file's `namespace` changes;
/// - `use` imports and fully qualified references elsewhere follow;
/// - files in the old namespace that used the class without an import get
///   one;
/// - the moved file gets imports for classes of its old namespace it used
///   by short name.
///
/// # Errors
/// Returns a message when the file declares no class.
pub fn move_class(
    index: &Index,
    path: &Path,
    src: &str,
    new_namespace: &str,
    new_path: Option<PathBuf>,
    load: impl Fn(&Path) -> Option<String>,
) -> Result<RenamePlan, String> {
    let new_namespace = new_namespace.trim().trim_matches('\\');
    let tree = parse(src);
    let root = tree.root_node();
    let mut c = root.walk();
    let children: Vec<Node> = root.named_children(&mut c).collect();
    let class = children
        .iter()
        .find(|n| is_class_like(n.kind()))
        .ok_or("The file declares no class")?;
    let short = class
        .child_by_field_name("name")
        .map(|n| text(n, src))
        .ok_or("The class has no name")?;
    let old_namespace = context_at(root, src, class.start_byte()).namespace;
    let old_fqn = if old_namespace.is_empty() {
        short.to_string()
    } else {
        format!("{old_namespace}\\{short}")
    };
    let new_fqn = if new_namespace.is_empty() {
        short.to_string()
    } else {
        format!("{new_namespace}\\{short}")
    };
    let mut plan = RenamePlan::default();
    let own = plan.edits.entry(path.to_path_buf()).or_default();
    match children
        .iter()
        .find(|n| n.kind() == "namespace_definition")
        .and_then(|n| n.child_by_field_name("name"))
    {
        Some(name) => own.push(Edit {
            path: Some(path.to_path_buf()),
            start: name.start_byte(),
            end: name.end_byte(),
            text: new_namespace.to_string(),
        }),
        None => {
            let at = import_insertion_point(root, class.start_byte());
            own.push(Edit {
                path: Some(path.to_path_buf()),
                start: at,
                end: at,
                text: format!("\n\nnamespace {new_namespace};"),
            });
        }
    }
    // Classes of the old namespace the moved file names without importing.
    let siblings: BTreeSet<String> = index
        .top_level()
        .filter(|(_, s)| s.kind.is_class_like())
        .filter_map(|(_, s)| {
            s.fqn
                .rsplit_once('\\')
                .filter(|(ns, _)| ns.eq_ignore_ascii_case(&old_namespace))
                .map(|(_, n)| n.to_string())
        })
        .filter(|n| !n.eq_ignore_ascii_case(short))
        .collect();
    let ctx = context_at(root, src, class.start_byte());
    let mut needed = BTreeSet::new();
    let mut stack = vec![*class];
    while let Some(n) = stack.pop() {
        if n.kind() == "name"
            && n.parent().is_some_and(|p| {
                !matches!(p.kind(), "qualified_name" | "namespace_name") && !is_class_like(p.kind())
            })
            && siblings.contains(text(n, src))
            && !ctx.classes.contains_key(&text(n, src).to_ascii_lowercase())
        {
            needed.insert(text(n, src).to_string());
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    if !needed.is_empty() && !old_namespace.is_empty() {
        let at = import_insertion_point(root, class.start_byte());
        let imports: String = needed
            .iter()
            .map(|n| format!("\nuse {old_namespace}\\{n};"))
            .collect();
        own.push(Edit {
            path: Some(path.to_path_buf()),
            start: at,
            end: at,
            text: imports,
        });
    }
    // References elsewhere.
    let defs: Vec<Definition> = index
        .classes(&old_fqn)
        .into_iter()
        .map(|(p, s)| Definition {
            path: p.to_path_buf(),
            symbol: s.clone(),
        })
        .collect();
    let mut imported: BTreeSet<PathBuf> = BTreeSet::new();
    for usage in find_references(index, &defs, &load)
        .into_iter()
        .filter(|u| u.path != path)
    {
        let Some(other) = load(&usage.path) else {
            continue;
        };
        let lines = LineIndex::new(&other);
        let at = lines.offset(&other, usage.span.start);
        let tree = parse(&other);
        let Some(node) = tree.root_node().descendant_for_byte_range(at, at) else {
            continue;
        };
        let mut full = node;
        while let Some(p) = full
            .parent()
            .filter(|p| matches!(p.kind(), "qualified_name" | "namespace_name"))
        {
            full = p;
        }
        let written = text(full, &other);
        let in_use = {
            let mut cur = Some(full);
            let mut found = false;
            while let Some(n) = cur {
                if matches!(
                    n.kind(),
                    "namespace_use_clause" | "namespace_use_declaration"
                ) {
                    found = true;
                    break;
                }
                cur = n.parent();
            }
            found
        };
        let edits = plan.edits.entry(usage.path.clone()).or_default();
        if written.contains('\\') {
            let leading = if written.starts_with('\\') { "\\" } else { "" };
            edits.push(Edit {
                path: Some(usage.path.clone()),
                start: full.start_byte(),
                end: full.end_byte(),
                text: format!("{leading}{new_fqn}"),
            });
        } else if !in_use {
            let ctx = context_at(tree.root_node(), &other, at);
            let same_old_namespace = ctx.namespace.eq_ignore_ascii_case(&old_namespace);
            if same_old_namespace
                && !ctx.classes.contains_key(&written.to_ascii_lowercase())
                && imported.insert(usage.path.clone())
            {
                let point = import_insertion_point(tree.root_node(), at);
                edits.push(Edit {
                    path: Some(usage.path.clone()),
                    start: point,
                    end: point,
                    text: format!("\nuse {new_fqn};"),
                });
            }
        }
    }
    for edits in plan.edits.values_mut() {
        edits.sort_by_key(|e| e.start);
        edits.dedup_by_key(|e| e.start);
    }
    plan.file_rename = new_path
        .filter(|p| p != path)
        .map(|p| (path.to_path_buf(), p));
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::index_source;
    use crate::refactor::apply_edits;
    use std::collections::HashMap;

    fn project(files: &[(&str, &str)]) -> (Index, HashMap<PathBuf, String>) {
        let mut index = Index::default();
        let mut texts = HashMap::new();
        for (path, src) in files {
            index.update(PathBuf::from(path), index_source(src));
            texts.insert(PathBuf::from(path), src.to_string());
        }
        (index, texts)
    }

    fn apply(plan: &RenamePlan, texts: &HashMap<PathBuf, String>, path: &str) -> String {
        let p = PathBuf::from(path);
        apply_edits(
            &texts[&p],
            plan.edits.get(&p).map(Vec::as_slice).unwrap_or_default(),
        )
    }

    #[test]
    fn change_signature_should_reorder_and_add_arguments() {
        let lib = "<?php\nfunction greet(string $name, int $times) {}\n";
        let main = "<?php\ngreet('ana', 2);\ngreet('bo', 3);\n";
        let (index, texts) = project(&[("/p/lib.php", lib), ("/p/main.php", main)]);
        let params = vec![
            ParamChange {
                from: Some(1),
                declaration: "int $times".into(),
                default_argument: String::new(),
            },
            ParamChange {
                from: Some(0),
                declaration: "string $name".into(),
                default_argument: String::new(),
            },
            ParamChange {
                from: None,
                declaration: "bool $loud = false".into(),
                default_argument: "true".into(),
            },
        ];
        let plan = change_signature(&index, main, main.find("greet").unwrap(), &params, |p| {
            texts.get(p).cloned()
        })
        .unwrap();
        assert_eq!(
            apply(&plan, &texts, "/p/lib.php"),
            "<?php\nfunction greet(int $times, string $name, bool $loud = false) {}\n"
        );
        assert_eq!(
            apply(&plan, &texts, "/p/main.php"),
            "<?php\ngreet(2, 'ana', true);\ngreet(3, 'bo', true);\n"
        );
    }

    #[test]
    fn move_class_should_update_namespace_imports_and_references() {
        let user = "<?php\nnamespace App;\n\nclass User extends Model {}\n";
        let model = "<?php\nnamespace App;\n\nclass Model {}\n";
        let ctrl = "<?php\nnamespace App\\Http;\n\nuse App\\User;\n\nclass C { public function f(User $u) { return new \\App\\User(); } }\n";
        let sibling = "<?php\nnamespace App;\n\nclass Repo { public function f(User $u) {} }\n";
        let (index, texts) = project(&[
            ("/p/User.php", user),
            ("/p/Model.php", model),
            ("/p/C.php", ctrl),
            ("/p/Repo.php", sibling),
        ]);
        let plan = move_class(
            &index,
            Path::new("/p/User.php"),
            user,
            "App\\Models",
            Some(PathBuf::from("/p/Models/User.php")),
            |p| texts.get(p).cloned(),
        )
        .unwrap();
        let moved = apply(&plan, &texts, "/p/User.php");
        assert!(moved.contains("namespace App\\Models;"), "{moved}");
        assert!(moved.contains("use App\\Model;"), "{moved}");
        let c = apply(&plan, &texts, "/p/C.php");
        assert!(
            c.contains("use App\\Models\\User;") && c.contains("new \\App\\Models\\User()"),
            "{c}"
        );
        let repo = apply(&plan, &texts, "/p/Repo.php");
        assert!(repo.contains("use App\\Models\\User;"), "{repo}");
        assert_eq!(
            plan.file_rename,
            Some((
                PathBuf::from("/p/User.php"),
                PathBuf::from("/p/Models/User.php")
            ))
        );
    }
}
