//! The project keys beyond routes, views, config, translations and env:
//! middleware, abilities, guards and other config-declared names, Pennant
//! features, Inertia pages, Livewire components, Blade stacks and sections,
//! assets and Artisan commands. Each scanner reads files only and skips
//! what it cannot read.

use super::*;
use crate::syntax::{LineIndex, Pos, parse, text};
use std::path::Path;
use tree_sitter::Node;

/// Middleware aliases and groups the framework registers by default.
const DEFAULT_MIDDLEWARE: &[(&str, &str)] = &[
    ("auth", "Authenticate"),
    ("auth.basic", "AuthenticateWithBasicAuth"),
    ("auth.session", "AuthenticateSession"),
    ("cache.headers", "SetCacheHeaders"),
    ("can", "Authorize"),
    ("guest", "RedirectIfAuthenticated"),
    ("password.confirm", "RequirePassword"),
    ("precognitive", "HandlePrecognitiveRequests"),
    ("signed", "ValidateSignature"),
    ("subscribed", "EnsureUserIsSubscribed"),
    ("throttle", "ThrottleRequests"),
    ("verified", "EnsureEmailIsVerified"),
    ("web", "middleware group"),
    ("api", "middleware group"),
];

/// Artisan commands every Laravel application has (the common ones).
const DEFAULT_COMMANDS: &[&str] = &[
    "about",
    "cache:clear",
    "cache:forget",
    "cache:prune-stale-tags",
    "config:cache",
    "config:clear",
    "db:seed",
    "db:wipe",
    "down",
    "env",
    "event:cache",
    "event:clear",
    "key:generate",
    "migrate",
    "migrate:fresh",
    "migrate:install",
    "migrate:refresh",
    "migrate:reset",
    "migrate:rollback",
    "migrate:status",
    "model:prune",
    "optimize",
    "optimize:clear",
    "queue:clear",
    "queue:failed",
    "queue:flush",
    "queue:forget",
    "queue:prune-batches",
    "queue:prune-failed",
    "queue:restart",
    "queue:retry",
    "queue:work",
    "route:cache",
    "route:clear",
    "schedule:run",
    "schedule:work",
    "storage:link",
    "up",
    "view:cache",
    "view:clear",
];

/// Config arrays whose keys name things code selects by string: the key
/// prefix, and the kind of the names under it.
const CONFIG_NAMES: &[(&str, KeyKind)] = &[
    ("auth.guards.", KeyKind::Guard),
    ("filesystems.disks.", KeyKind::Disk),
    ("cache.stores.", KeyKind::CacheStore),
    ("queue.connections.", KeyKind::QueueConnection),
    ("logging.channels.", KeyKind::LogChannel),
    ("database.connections.", KeyKind::DbConnection),
    ("mail.mailers.", KeyKind::Mailer),
];

/// Policy methods that are hooks, not abilities.
const POLICY_HOOKS: &[&str] = &["before", "after", "__construct"];

/// Folders Inertia page components live in, relative to the project root.
const INERTIA_PAGES: &[&str] = &["resources/js/Pages", "resources/js/pages"];

/// File types Inertia pages are written in.
const PAGE_EXTENSIONS: &[&str] = &["vue", "jsx", "tsx", "svelte", "js", "ts"];

/// Source folders Vite usually builds from.
const VITE_SOURCES: &[&str] = &[
    "resources/js",
    "resources/css",
    "resources/sass",
    "resources/scss",
    "resources/ts",
];

/// File types Vite takes as entry points.
const VITE_EXTENSIONS: &[&str] = &[
    "js", "ts", "jsx", "tsx", "vue", "css", "scss", "sass", "less",
];

/// Most public files listed for `asset()`, so huge `public` folders stay cheap.
const MAX_PUBLIC_FILES: usize = 2000;

/// Folders under `public` that hold generated files, not assets to name.
const PUBLIC_SKIP: &[&str] = &["build", "storage", "vendor", "hot"];

/// Adds every key kind this module knows to `project`. Needs the config
/// keys already scanned: guards, disks and the like are read from them.
pub(super) fn scan_catalog(root: &Path, project: &mut LaravelProject) {
    let config_names = config_names(project);
    for (kind, keys) in config_names {
        project.add(kind, keys);
    }
    project.add(KeyKind::Middleware, middleware(root));
    project.add(KeyKind::Ability, abilities(root));
    project.add(KeyKind::Feature, features(root));
    project.add(KeyKind::InertiaPage, inertia_pages(root));
    project.add(KeyKind::Livewire, livewire_components(root));
    let (stacks, sections) = stacks_and_sections(root);
    project.add(KeyKind::Stack, stacks);
    project.add(KeyKind::Section, sections);
    project.add(KeyKind::ViteAsset, vite_assets(root));
    project.add(KeyKind::Asset, public_assets(root));
    project.add(KeyKind::Command, commands(root));
    project.add(KeyKind::ValidationRule, super::rules::validation_rules());
    project.add(KeyKind::Binding, super::container::bindings());
}

/// A key with no meaningful position: the start of `path`.
fn key_at_file(name: String, path: &Path, detail: String) -> LaravelKey {
    LaravelKey {
        name,
        path: path.to_path_buf(),
        pos: Pos::default(),
        detail,
    }
}

/// Path of `path` relative to `base`, with forward slashes.
fn relative(path: &Path, base: &Path) -> Option<String> {
    path.strip_prefix(base)
        .ok()
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
}

/// `FooBar` -> `foo-bar` (Livewire names).
fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Guards, disks, cache stores, queue and database connections, log
/// channels and mailers: the direct children of their config arrays, with
/// the driver as detail when the config says it.
fn config_names(project: &LaravelProject) -> Vec<(KeyKind, Vec<LaravelKey>)> {
    let config = project.keys(KeyKind::Config);
    CONFIG_NAMES
        .iter()
        .map(|(prefix, kind)| {
            let keys = config
                .iter()
                .filter_map(|k| {
                    let name = k.name.strip_prefix(prefix)?;
                    if name.contains('.') {
                        return None;
                    }
                    let driver = format!("{prefix}{name}.driver");
                    let detail = config
                        .iter()
                        .find(|d| d.name == driver)
                        .map(|d| d.detail.trim_matches(['\'', '"']).to_string())
                        .unwrap_or_else(|| k.name.clone());
                    Some(LaravelKey {
                        name: name.to_string(),
                        path: k.path.clone(),
                        pos: k.pos,
                        detail,
                    })
                })
                .collect();
            (*kind, keys)
        })
        .collect()
}

/// Every node of `tree` under `root`, depth first.
fn descendants(root: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let mut c = node.walk();
        stack.extend(node.named_children(&mut c));
        out.push(node);
    }
    out
}

/// Method name of a call node (`->alias(…)`, `::define(…)`).
fn call_name<'s>(node: Node, src: &'s str) -> Option<&'s str> {
    matches!(
        node.kind(),
        "member_call_expression" | "scoped_call_expression" | "nullsafe_member_call_expression"
    )
    .then(|| node.child_by_field_name("name").map(|n| text(n, src)))
    .flatten()
}

/// The first argument of a call when it is an array literal.
fn array_argument(call: Node) -> Option<Node> {
    let args = call.child_by_field_name("arguments")?;
    let mut c = args.walk();
    let arg = args
        .named_children(&mut c)
        .find(|a| a.kind() == "argument")?;
    arg.named_child(0)
        .filter(|v| v.kind() == "array_creation_expression")
}

/// String keys of an array literal, with the key node.
fn keyed_entries<'t>(array: Node<'t>, src: &str) -> Vec<(String, Node<'t>)> {
    let mut c = array.walk();
    array
        .named_children(&mut c)
        .filter(|e| e.kind() == "array_element_initializer")
        .filter_map(|e| {
            let key = e.named_child(0)?;
            e.named_child(1)?;
            Some((string_value(key, src)?, key))
        })
        .collect()
}

/// Middleware aliases and groups: the framework's, plus those registered
/// in `bootstrap/app.php` (`->alias([...])`, `->group('name', …)`,
/// `->appendToGroup('name', …)`) or an HTTP kernel's
/// `$middlewareAliases` / `$routeMiddleware` / `$middlewareGroups`, and a
/// `throttle:<name>` entry per rate limiter.
fn middleware(root: &Path) -> Vec<LaravelKey> {
    let bootstrap = root.join("bootstrap/app.php");
    let mut out: Vec<LaravelKey> = DEFAULT_MIDDLEWARE
        .iter()
        .map(|(name, detail)| key_at_file((*name).to_string(), &bootstrap, (*detail).to_string()))
        .collect();
    let kernel = root.join("app/Http/Kernel.php");
    for path in [bootstrap, kernel] {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let tree = parse(&src);
        let lines = LineIndex::new(&src);
        for node in descendants(tree.root_node()) {
            let array = match node.kind() {
                "property_element" => {
                    let name = node.named_child(0).map(|n| text(n, &src));
                    if !matches!(
                        name,
                        Some("$middlewareAliases" | "$routeMiddleware" | "$middlewareGroups")
                    ) {
                        continue;
                    }
                    let Some(value) = node
                        .named_child(1)
                        .and_then(|v| {
                            if v.kind() == "property_initializer" {
                                v.named_child(0)
                            } else {
                                Some(v)
                            }
                        })
                        .filter(|v| v.kind() == "array_creation_expression")
                    else {
                        continue;
                    };
                    value
                }
                _ => match call_name(node, &src) {
                    Some("alias") => {
                        let Some(array) = array_argument(node) else {
                            continue;
                        };
                        array
                    }
                    Some("group" | "appendToGroup" | "prependToGroup") => {
                        if let Some((name, value)) = string_argument(node, &src, 0) {
                            out.push(LaravelKey {
                                name,
                                path: path.clone(),
                                pos: lines.pos(&src, value.start_byte() + 1),
                                detail: "middleware group".to_string(),
                            });
                        }
                        continue;
                    }
                    _ => continue,
                },
            };
            for (name, key) in keyed_entries(array, &src) {
                out.push(LaravelKey {
                    name,
                    path: path.clone(),
                    pos: lines.pos(&src, key.start_byte() + 1),
                    detail: "middleware".to_string(),
                });
            }
        }
    }
    for limiter in string_calls(
        root,
        &["app", "bootstrap", "routes"],
        "RateLimiter",
        &["for"],
    ) {
        out.push(LaravelKey {
            name: format!("throttle:{}", limiter.name),
            detail: "rate limiter".to_string(),
            ..limiter
        });
    }
    out
}

/// First string arguments of `Class::method('…')` calls in the PHP files
/// under `dirs`, for any of `methods`.
fn string_calls(root: &Path, dirs: &[&str], class: &str, methods: &[&str]) -> Vec<LaravelKey> {
    let needle = format!("{class}::");
    let mut out = Vec::new();
    for dir in dirs {
        for path in files_under(&root.join(dir), ".php") {
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !src.contains(&needle) {
                continue;
            }
            let tree = parse(&src);
            let lines = LineIndex::new(&src);
            for node in descendants(tree.root_node()) {
                let scope = node.child_by_field_name("scope").map(|n| text(n, &src));
                let is_class = scope
                    .is_some_and(|s| s.trim_start_matches('\\').rsplit('\\').next() == Some(class));
                if node.kind() != "scoped_call_expression" || !is_class {
                    continue;
                }
                if !call_name(node, &src).is_some_and(|m| methods.contains(&m)) {
                    continue;
                }
                if let Some((name, value)) = string_argument(node, &src, 0) {
                    out.push(LaravelKey {
                        name,
                        path: path.clone(),
                        pos: lines.pos(&src, value.start_byte() + 1),
                        detail: format!("{class}::{}", call_name(node, &src).unwrap_or_default()),
                    });
                }
            }
        }
    }
    out
}

/// Gate abilities (`Gate::define('edit-post', …)` anywhere under `app`)
/// and policy abilities (public methods of `app/Policies` classes).
fn abilities(root: &Path) -> Vec<LaravelKey> {
    let mut out = string_calls(root, &["app", "bootstrap", "routes"], "Gate", &["define"]);
    for key in &mut out {
        key.detail = "gate".to_string();
    }
    let policies = root.join("app/Policies");
    for path in files_under(&policies, ".php") {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let class = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tree = parse(&src);
        let lines = LineIndex::new(&src);
        for node in descendants(tree.root_node()) {
            if node.kind() != "method_declaration" {
                continue;
            }
            let public = {
                let mut c = node.walk();
                node.children(&mut c)
                    .all(|m| m.kind() != "visibility_modifier" || text(m, &src) == "public")
            };
            let Some(name) = node.child_by_field_name("name") else {
                continue;
            };
            let method = text(name, &src);
            if !public || POLICY_HOOKS.contains(&method) {
                continue;
            }
            out.push(LaravelKey {
                name: method.to_string(),
                path: path.clone(),
                pos: lines.pos(&src, name.start_byte()),
                detail: class.clone(),
            });
        }
    }
    out
}

/// Pennant features defined with `Feature::define('…')`.
fn features(root: &Path) -> Vec<LaravelKey> {
    string_calls(
        root,
        &["app", "bootstrap", "routes"],
        "Feature",
        &["define"],
    )
}

/// Inertia page components, named by their path under the pages folder
/// without extension (`Users/Index`).
fn inertia_pages(root: &Path) -> Vec<LaravelKey> {
    let mut out = Vec::new();
    for dir in INERTIA_PAGES {
        let base = root.join(dir);
        for path in files_under(&base, "") {
            let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
            if !ext.is_some_and(|e| PAGE_EXTENSIONS.contains(&e.as_str())) {
                continue;
            }
            let Some(rel) = relative(&path, &base) else {
                continue;
            };
            let name = rel
                .rsplit_once('.')
                .map_or(rel.as_str(), |(n, _)| n)
                .to_string();
            out.push(key_at_file(name, &path, format!("{dir}/{rel}")));
        }
    }
    out
}

/// Livewire components: classes under `app/Livewire` (v3) or
/// `app/Http/Livewire` (v2), named by their kebab-cased dotted path
/// (`Users/DataTable.php` -> `users.data-table`), and Volt components
/// under `resources/views/livewire` and `resources/views/pages`.
fn livewire_components(root: &Path) -> Vec<LaravelKey> {
    let mut out = Vec::new();
    for dir in ["app/Livewire", "app/Http/Livewire"] {
        let base = root.join(dir);
        for path in files_under(&base, ".php") {
            let Some(rel) = relative(&path, &base) else {
                continue;
            };
            let name = rel
                .trim_end_matches(".php")
                .split('/')
                .map(kebab)
                .collect::<Vec<_>>()
                .join(".");
            out.push(key_at_file(name, &path, format!("{dir}/{rel}")));
        }
    }
    for dir in ["resources/views/livewire", "resources/views/pages"] {
        let base = root.join(dir);
        for path in files_under(&base, ".blade.php") {
            let is_volt =
                std::fs::read_to_string(&path).is_ok_and(|s| s.contains("Livewire\\Volt"));
            let Some(rel) = relative(&path, &base).filter(|_| is_volt) else {
                continue;
            };
            let name = rel.trim_end_matches(".blade.php").replace('/', ".");
            let name = if dir.ends_with("pages") {
                format!("pages.{name}")
            } else {
                name
            };
            out.push(key_at_file(name, &path, format!("{dir}/{rel} (Volt)")));
        }
    }
    out
}

/// String arguments of `@directive('…')` calls in a Blade source, with the
/// offset of the string's content.
pub(super) fn directive_strings<'s>(src: &'s str, directive: &str) -> Vec<(usize, &'s str)> {
    let pattern = format!("@{directive}(");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = src[from..].find(&pattern) {
        let at = from + i;
        from = at + pattern.len();
        // `@pushOnce(` must not match `@push(`'s pattern and vice versa:
        // the pattern ends at `(`, so only exact names match.
        if src[..at].ends_with('@') {
            continue;
        }
        let rest = src[from..].trim_start();
        let Some(quote) = rest.chars().next().filter(|c| *c == '\'' || *c == '"') else {
            continue;
        };
        let start = src.len() - rest.len() + 1;
        let Some(len) = src[start..].find(quote) else {
            continue;
        };
        out.push((start, &src[start..start + len]));
    }
    out
}

/// Stack names (`@stack('scripts')`) and section names (`@yield('title')`,
/// `@section('title')`) used across the project's views.
fn stacks_and_sections(root: &Path) -> (Vec<LaravelKey>, Vec<LaravelKey>) {
    let views = root.join("resources/views");
    let (mut stacks, mut sections) = (Vec::new(), Vec::new());
    for path in files_under(&views, ".blade.php") {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        if !src.contains("@stack(") && !src.contains("@yield(") && !src.contains("@section(") {
            continue;
        }
        let lines = LineIndex::new(&src);
        let rel = relative(&path, root).unwrap_or_default();
        let push = |out: &mut Vec<LaravelKey>, directive: &str| {
            for (at, name) in directive_strings(&src, directive) {
                out.push(LaravelKey {
                    name: name.to_string(),
                    path: path.clone(),
                    pos: lines.pos(&src, at),
                    detail: rel.clone(),
                });
            }
        };
        push(&mut stacks, "stack");
        push(&mut sections, "yield");
        push(&mut sections, "section");
    }
    (stacks, sections)
}

/// Entry points for `@vite`: the source files under the usual folders,
/// as paths from the project root (`resources/js/app.js`).
fn vite_assets(root: &Path) -> Vec<LaravelKey> {
    let mut out = Vec::new();
    for dir in VITE_SOURCES {
        for path in files_under(&root.join(dir), "") {
            let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
            if !ext.is_some_and(|e| VITE_EXTENSIONS.contains(&e.as_str())) {
                continue;
            }
            if let Some(rel) = relative(&path, root) {
                out.push(key_at_file(rel, &path, "Vite".to_string()));
            }
        }
    }
    out
}

/// Files under `public`, as `asset()` takes them (`css/app.css`); build
/// output and links are skipped, and the list is capped.
fn public_assets(root: &Path) -> Vec<LaravelKey> {
    let public = root.join("public");
    walkdir::WalkDir::new(&public)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') && !(e.depth() == 1 && PUBLIC_SKIP.contains(&name.as_ref()))
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name() != "index.php")
        .take(MAX_PUBLIC_FILES)
        .filter_map(|e| {
            let rel = relative(e.path(), &public)?;
            Some(key_at_file(rel, e.path(), "public".to_string()))
        })
        .collect()
}

/// Artisan commands: the framework's common ones, the project's command
/// classes (`$signature` / `$name` / `#[AsCommand(name: …)]`) and closure
/// commands in `routes/console.php`.
fn commands(root: &Path) -> Vec<LaravelKey> {
    let artisan = root.join("artisan");
    let mut out: Vec<LaravelKey> = DEFAULT_COMMANDS
        .iter()
        .map(|c| key_at_file((*c).to_string(), &artisan, "Laravel".to_string()))
        .collect();
    for path in files_under(&root.join("app/Console/Commands"), ".php") {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let lines = LineIndex::new(&src);
        let class = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tree = parse(&src);
        for node in descendants(tree.root_node()) {
            let value = match node.kind() {
                "property_element" => {
                    let name = node.named_child(0).map(|n| text(n, &src));
                    if !matches!(name, Some("$signature" | "$name")) {
                        continue;
                    }
                    node.named_child(1).and_then(|v| {
                        if v.kind() == "property_initializer" {
                            v.named_child(0)
                        } else {
                            Some(v)
                        }
                    })
                }
                "argument"
                    if node
                        .child_by_field_name("name")
                        .is_some_and(|n| text(n, &src) == "name")
                        && node.parent().and_then(|p| p.parent()).is_some_and(|a| {
                            a.kind() == "attribute" && text(a, &src).starts_with("AsCommand")
                        }) =>
                {
                    node.named_child(
                        u32::try_from(node.named_child_count().saturating_sub(1)).unwrap_or(0),
                    )
                }
                _ => continue,
            };
            let Some(value) = value else { continue };
            let Some(signature) = string_value(value, &src) else {
                continue;
            };
            let Some(name) = signature.split_whitespace().next() else {
                continue;
            };
            out.push(LaravelKey {
                name: name.to_string(),
                path: path.clone(),
                pos: lines.pos(&src, value.start_byte() + 1),
                detail: class.clone(),
            });
        }
    }
    let console = root.join("routes/console.php");
    if let Ok(src) = std::fs::read_to_string(&console) {
        let lines = LineIndex::new(&src);
        let tree = parse(&src);
        for node in descendants(tree.root_node()) {
            if call_name(node, &src) != Some("command") {
                continue;
            }
            if let Some((signature, value)) = string_argument(node, &src, 0)
                && let Some(name) = signature.split_whitespace().next()
            {
                out.push(LaravelKey {
                    name: name.to_string(),
                    path: console.clone(),
                    pos: lines.pos(&src, value.start_byte() + 1),
                    detail: "routes/console.php".to_string(),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directive_strings_should_read_exact_directives() {
        let src = "@push('scripts') @pushOnce('once') @stack(\"styles\") @@stack('no')";
        let pushed: Vec<&str> = directive_strings(src, "push")
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        assert_eq!(pushed, ["scripts"]);
        let stacks: Vec<&str> = directive_strings(src, "stack")
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        assert_eq!(stacks, ["styles"]);
    }

    #[test]
    fn config_names_should_list_direct_children_with_driver() {
        let mut project = LaravelProject::default();
        let src = "<?php return ['disks' => ['local' => ['driver' => 'local'], 's3' => ['driver' => 's3']]];";
        project.add(
            KeyKind::Config,
            config_keys(src, Path::new("config/filesystems.php")),
        );
        let names = config_names(&project);
        let disks = &names.iter().find(|(k, _)| *k == KeyKind::Disk).unwrap().1;
        let found: Vec<(&str, &str)> = disks
            .iter()
            .map(|k| (k.name.as_str(), k.detail.as_str()))
            .collect();
        assert_eq!(found, [("local", "local"), ("s3", "s3")]);
    }

    #[test]
    fn kebab_should_split_words() {
        assert_eq!(kebab("DataTable"), "data-table");
    }
}
