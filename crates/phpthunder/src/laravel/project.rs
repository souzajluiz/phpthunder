//! Scanning a Laravel project for its route names, views and config keys.

use super::*;
use crate::syntax::{LineIndex, Pos, parse, text};
use std::path::Path;
use tree_sitter::Node;

/// Route names `Route::resource` registers, per action.
const RESOURCE_ACTIONS: &[&str] = &[
    "index", "create", "store", "show", "edit", "update", "destroy",
];

/// Actions of `Route::apiResource` (no forms).
const API_RESOURCE_ACTIONS: &[&str] = &["index", "store", "show", "update", "destroy"];

/// Deepest nesting of config keys collected (`app.providers.x` is 3).
const CONFIG_DEPTH: usize = 3;

/// Whether `root` looks like a Laravel application (has `artisan`).
pub fn is_laravel(root: &Path) -> bool {
    root.join("artisan").is_file()
}

/// Route names, views and config keys of the Laravel project at `root`;
/// `None` when it is not a Laravel project. Reads files only; unreadable
/// files are skipped. Run off the UI thread: it walks `resources/views`.
pub fn scan_project(root: &Path) -> Option<LaravelProject> {
    if !is_laravel(root) {
        return None;
    }
    let mut project = LaravelProject::default();
    for path in files_under(&root.join("routes"), ".php") {
        if let Ok(src) = std::fs::read_to_string(&path) {
            project.routes.extend(route_names(&src, &path));
        }
    }
    let views = root.join("resources").join("views");
    for path in files_under(&views, ".blade.php") {
        let Ok(rel) = path.strip_prefix(&views) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let name = rel.trim_end_matches(".blade.php").replace('/', ".");
        project.views.push(LaravelKey {
            name,
            path: path.clone(),
            pos: Pos::default(),
            detail: format!("resources/views/{rel}"),
        });
    }
    for path in files_under(&root.join("config"), ".php") {
        if let Ok(src) = std::fs::read_to_string(&path) {
            project.config.extend(config_keys(&src, &path));
        }
    }
    for keys in [&mut project.routes, &mut project.views, &mut project.config] {
        keys.sort_by(|a, b| a.name.cmp(&b.name));
        keys.dedup_by(|a, b| a.name == b.name);
    }
    Some(project)
}

/// Files under `dir` (recursively) whose name ends with `suffix`.
fn files_under(dir: &Path, suffix: &str) -> Vec<std::path::PathBuf> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name().to_string_lossy().ends_with(suffix))
        .map(|e| e.into_path())
        .collect()
}

/// Contents of a string literal node, quotes removed.
fn string_value(node: Node, src: &str) -> Option<String> {
    matches!(node.kind(), "string" | "encapsed_string").then(|| {
        text(node, src)
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string()
    })
}

/// String value of the `i`-th argument of a call node.
fn string_argument<'t>(call: Node<'t>, src: &str, i: usize) -> Option<(String, Node<'t>)> {
    let args = call.child_by_field_name("arguments")?;
    let mut c = args.walk();
    let arg = args
        .named_children(&mut c)
        .filter(|a| a.kind() == "argument")
        .nth(i)?;
    let value = arg.named_child(0)?;
    Some((string_value(value, src)?, value))
}

/// Route names declared in one routes file: `->name('x')` on a route
/// (group prefixes ending in `.` are skipped) and the names
/// `Route::resource` / `Route::apiResource` generate.
pub fn route_names(src: &str, path: &Path) -> Vec<LaravelKey> {
    let tree = parse(src);
    let lines = LineIndex::new(src);
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        let mut c = node.walk();
        stack.extend(node.named_children(&mut c));
        let name = |field: &str| node.child_by_field_name(field).map(|n| text(n, src));
        match node.kind() {
            "member_call_expression" if name("name") == Some("name") => {
                let Some((route, value)) = string_argument(node, src, 0) else {
                    continue;
                };
                if route.is_empty() || route.ends_with('.') {
                    continue;
                }
                out.push(LaravelKey {
                    name: format!("{}{route}", group_prefix(node, src)),
                    path: path.to_path_buf(),
                    pos: lines.pos(src, value.start_byte() + 1),
                    detail: route_uri(node, src).unwrap_or_default(),
                });
            }
            "scoped_call_expression" if name("scope") == Some("Route") => {
                let actions = match name("name") {
                    Some("resource") => RESOURCE_ACTIONS,
                    Some("apiResource") => API_RESOURCE_ACTIONS,
                    _ => continue,
                };
                let Some((resource, value)) = string_argument(node, src, 0) else {
                    continue;
                };
                let pos = lines.pos(src, value.start_byte() + 1);
                let prefix = group_prefix(node, src);
                out.extend(actions.iter().map(|action| LaravelKey {
                    name: format!("{prefix}{resource}.{action}"),
                    path: path.to_path_buf(),
                    pos,
                    detail: format!("/{resource}"),
                }));
            }
            _ => {}
        }
    }
    out
}

/// Name prefixes of the route groups enclosing `node`, outermost first:
/// `Route::name('admin.')->group(…)` and `->as('admin.')`.
fn group_prefix(node: Node, src: &str) -> String {
    let mut prefixes = Vec::new();
    let mut current = node.parent();
    while let Some(n) = current {
        let is_group = matches!(
            n.kind(),
            "member_call_expression" | "scoped_call_expression"
        ) && n
            .child_by_field_name("name")
            .is_some_and(|m| text(m, src) == "group");
        if is_group {
            let mut link = n
                .child_by_field_name("object")
                .or_else(|| n.child_by_field_name("scope"));
            let mut prefix = String::new();
            while let Some(call) = link.filter(|l| {
                matches!(
                    l.kind(),
                    "member_call_expression" | "scoped_call_expression"
                )
            }) {
                let method = call.child_by_field_name("name").map(|m| text(m, src));
                if matches!(method, Some("name" | "as"))
                    && let Some((p, _)) = string_argument(call, src, 0)
                {
                    prefix = p + &prefix;
                }
                link = call.child_by_field_name("object");
            }
            prefixes.push(prefix);
        }
        current = n.parent();
    }
    prefixes.reverse();
    prefixes.concat()
}

/// The URI of the `Route::get('/uri', …)` call a `->name()` chain starts
/// from, for the completion detail.
fn route_uri(mut node: Node, src: &str) -> Option<String> {
    while node.kind() == "member_call_expression" {
        node = node.child_by_field_name("object")?;
    }
    (node.kind() == "scoped_call_expression")
        .then(|| string_argument(node, src, 0).map(|(uri, _)| uri))
        .flatten()
}

/// Dotted keys of a config file's returned array (`config/app.php` gives
/// `app.name`, `app.providers`, …), nested up to [`CONFIG_DEPTH`].
pub fn config_keys(src: &str, path: &Path) -> Vec<LaravelKey> {
    let Some(file) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
        return Vec::new();
    };
    let tree = parse(src);
    let lines = LineIndex::new(src);
    let mut out = Vec::new();
    let mut c = tree.root_node().walk();
    let returned = tree
        .root_node()
        .named_children(&mut c)
        .filter(|n| n.kind() == "return_statement")
        .find_map(|r| r.named_child(0))
        .filter(|n| n.kind() == "array_creation_expression");
    if let Some(array) = returned {
        collect_config(array, src, &lines, path, &file, 1, &mut out);
    }
    out
}

/// Adds the string keys of `array` under `prefix`, recursing into nested arrays.
fn collect_config(
    array: Node,
    src: &str,
    lines: &LineIndex,
    path: &Path,
    prefix: &str,
    depth: usize,
    out: &mut Vec<LaravelKey>,
) {
    let mut c = array.walk();
    for element in array
        .named_children(&mut c)
        .filter(|e| e.kind() == "array_element_initializer")
    {
        let (Some(key), Some(value)) = (element.named_child(0), element.named_child(1)) else {
            continue;
        };
        let Some(name) = string_value(key, src) else {
            continue;
        };
        let full = format!("{prefix}.{name}");
        let raw = text(value, src)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let detail = if raw.chars().count() > 48 {
            format!("{}…", raw.chars().take(47).collect::<String>())
        } else {
            raw
        };
        out.push(LaravelKey {
            name: full.clone(),
            path: path.to_path_buf(),
            pos: lines.pos(src, key.start_byte() + 1),
            detail,
        });
        if depth < CONFIG_DEPTH && value.kind() == "array_creation_expression" {
            collect_config(value, src, lines, path, &full, depth + 1, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_names_should_read_named_routes_and_resources() {
        let src = "<?php\nRoute::get('/users/{id}', [UserController::class, 'show'])->name('users.show');\nRoute::name('admin.')->group(function () { Route::get('/', fn () => 1)->name('home'); Route::resource('users', UserController::class); });\nRoute::apiResource('posts', PostController::class);\n";
        let routes = route_names(src, Path::new("routes/web.php"));
        let names: Vec<&str> = routes.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"users.show"));
        assert!(names.contains(&"posts.destroy"));
        assert!(!names.contains(&"posts.create"));
        assert!(!names.iter().any(|n| n.ends_with('.')));
        assert!(names.contains(&"admin.home"));
        assert!(names.contains(&"admin.users.edit"));
        let show = routes.iter().find(|r| r.name == "users.show").unwrap();
        assert_eq!(show.detail, "/users/{id}");
        assert_eq!(show.pos.line, 1);
    }

    #[test]
    fn config_keys_should_dot_nested_keys() {
        let src = "<?php\nreturn [\n    'name' => env('APP_NAME', 'Laravel'),\n    'mail' => ['from' => ['address' => 'a@b.c']],\n];\n";
        let keys = config_keys(src, Path::new("config/app.php"));
        let names: Vec<&str> = keys.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "app.name",
                "app.mail",
                "app.mail.from",
                "app.mail.from.address"
            ]
        );
        assert_eq!(keys[0].detail, "env('APP_NAME', 'Laravel')");
    }
}
