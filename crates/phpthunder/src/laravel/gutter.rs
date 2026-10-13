//! Gutter markers for Laravel files: in route files, a jump from each
//! controller action to its method; in migrations, seeders and Artisan
//! commands, a way to run them; on a model, a jump to the migration of its
//! table.

use crate::analysis::Analyzer;
use crate::index::Index;
use crate::symbol::{MemberTag, TABLE_PREFIX};
use crate::syntax::{LineIndex, Pos, parse, text};
use std::path::PathBuf;
use tree_sitter::Node;

/// What a marker stands for; front ends pick its icon and tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GutterKind {
    /// A route's controller action (`[UserController::class, 'show']`).
    RouteAction,
    /// A migration that can be run.
    RunMigration,
    /// A seeder that can be run.
    RunSeeder,
    /// An Artisan command class that can be run.
    RunCommand,
    /// A model, leading to the migration creating its table.
    ModelTable,
}

/// What clicking a marker does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GutterAction {
    /// Open a file at a position.
    Open {
        /// The file.
        path: PathBuf,
        /// Where in it.
        pos: Pos,
    },
    /// Run a command line in the terminal.
    Run(String),
}

/// One marker on a line of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GutterMarker {
    /// Line it sits on (0-based).
    pub line: u32,
    /// What it stands for.
    pub kind: GutterKind,
    /// What clicking it does.
    pub action: GutterAction,
    /// Short context for the tooltip: `UserController::show`, the command.
    pub detail: String,
}

/// The markers of the file at `rel` (path from the project root, with `/`).
pub fn gutter_markers(index: &Index, src: &str, rel: &str) -> Vec<GutterMarker> {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    let lines = LineIndex::new(src);
    let mut out = Vec::new();
    let root = tree.root_node();
    if rel.starts_with("routes/") {
        route_actions(&analyzer, root, &lines, &mut out);
    }
    if rel.starts_with("database/migrations/") && src.contains("Schema::") {
        let command = format!("php artisan migrate --path={rel}");
        out.extend(
            first_line_of(root, "class_declaration").map(|line| GutterMarker {
                line,
                kind: GutterKind::RunMigration,
                action: GutterAction::Run(command.clone()),
                detail: command.clone(),
            }),
        );
    }
    if rel.starts_with("database/seeders/")
        && let Some((line, class)) = class_name(root, src)
    {
        let command = format!("php artisan db:seed --class={class}");
        out.push(GutterMarker {
            line,
            kind: GutterKind::RunSeeder,
            action: GutterAction::Run(command.clone()),
            detail: command,
        });
    }
    if rel.starts_with("app/Console/Commands/") {
        command_marker(root, src, &lines, &mut out);
    }
    if rel.starts_with("app/") {
        model_marker(&analyzer, root, &lines, &mut out);
    }
    out.sort_by_key(|m| m.line);
    out
}

/// Every node under `root`, depth first.
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

/// Line of the first node of `kind`.
fn first_line_of(root: Node, kind: &str) -> Option<u32> {
    descendants(root)
        .into_iter()
        .filter(|n| n.kind() == kind)
        .min_by_key(Node::start_byte)
        .map(|n| u32::try_from(n.start_position().row).unwrap_or(u32::MAX))
}

/// Line and short name of the first class in the file.
fn class_name(root: Node, src: &str) -> Option<(u32, String)> {
    let class = descendants(root)
        .into_iter()
        .filter(|n| n.kind() == "class_declaration")
        .min_by_key(Node::start_byte)?;
    let name = text(class.child_by_field_name("name")?, src).to_string();
    Some((u32::try_from(class.start_position().row).ok()?, name))
}

/// `[Controller::class, 'method']`, `Controller::class` (invokable) and
/// `'Controller@method'` in route definitions, resolved to the method.
fn route_actions(analyzer: &Analyzer, root: Node, lines: &LineIndex, out: &mut Vec<GutterMarker>) {
    let src = analyzer.src;
    for node in descendants(root) {
        let (class_node, method) = match node.kind() {
            "array_creation_expression" => {
                let mut c = node.walk();
                let items: Vec<Node> = node
                    .named_children(&mut c)
                    .filter(|e| e.kind() == "array_element_initializer")
                    .filter_map(|e| e.named_child(0))
                    .collect();
                let [class, method] = items.as_slice() else {
                    continue;
                };
                if class.kind() != "class_constant_access_expression" || method.kind() != "string" {
                    continue;
                }
                (
                    *class,
                    text(*method, src).trim_matches(['\'', '"']).to_string(),
                )
            }
            // An invokable controller passed alone: `Route::get('/', Home::class)`.
            "class_constant_access_expression"
                if node.parent().is_some_and(|p| p.kind() == "argument")
                    && text(node, src).ends_with("::class") =>
            {
                (node, "__invoke".to_string())
            }
            _ => continue,
        };
        let raw = text(class_node, src).trim_end_matches("::class");
        let ctx = analyzer.ctx(class_node.start_byte());
        let class = ctx.resolve_class(raw);
        let Some((path, sym)) = analyzer
            .index
            .find_member(&class, MemberTag::Method, &method)
            .into_iter()
            .next()
        else {
            continue;
        };
        let short = class.rsplit('\\').next().unwrap_or(&class).to_string();
        out.push(GutterMarker {
            line: lines.pos(src, node.start_byte()).line,
            kind: GutterKind::RouteAction,
            action: GutterAction::Open {
                path: path.to_path_buf(),
                pos: sym.name_span.start,
            },
            detail: format!("{short}::{method}"),
        });
    }
}

/// A run marker on an Artisan command's `$signature` / `$name`.
fn command_marker(root: Node, src: &str, lines: &LineIndex, out: &mut Vec<GutterMarker>) {
    for node in descendants(root) {
        if node.kind() != "property_element" {
            continue;
        }
        let name = node.named_child(0).map(|n| text(n, src));
        if !matches!(name, Some("$signature" | "$name")) {
            continue;
        }
        let value = node
            .named_child(1)
            .map(|v| {
                if v.kind() == "property_initializer" {
                    v.named_child(0).unwrap_or(v)
                } else {
                    v
                }
            })
            .filter(|v| v.kind() == "string" || v.kind() == "encapsed_string");
        let Some(command) = value
            .map(|v| text(v, src).trim_matches(['\'', '"']).to_string())
            .and_then(|s| s.split_whitespace().next().map(str::to_string))
        else {
            continue;
        };
        let command = format!("php artisan {command}");
        out.push(GutterMarker {
            line: lines.pos(src, node.start_byte()).line,
            kind: GutterKind::RunCommand,
            action: GutterAction::Run(command.clone()),
            detail: command,
        });
        return;
    }
}

/// On a model class, a jump to the first migration column of its table.
fn model_marker(analyzer: &Analyzer, root: Node, lines: &LineIndex, out: &mut Vec<GutterMarker>) {
    let src = analyzer.src;
    for class in descendants(root)
        .into_iter()
        .filter(|n| n.kind() == "class_declaration")
    {
        let Some(name) = class.child_by_field_name("name") else {
            continue;
        };
        let fqn = analyzer.ctx(name.start_byte()).qualify(text(name, src));
        let Some(table) = analyzer
            .index
            .hierarchy(&fqn)
            .into_iter()
            .find(|c| c.starts_with(TABLE_PREFIX))
        else {
            continue;
        };
        let Some((path, sym)) = analyzer.index.members(&table).into_iter().next() else {
            continue;
        };
        out.push(GutterMarker {
            line: lines.pos(src, class.start_byte()).line,
            kind: GutterKind::ModelTable,
            action: GutterAction::Open {
                path: path.to_path_buf(),
                pos: sym.name_span.start,
            },
            detail: table.trim_start_matches(TABLE_PREFIX).to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::index_source;

    fn index() -> Index {
        let mut idx = Index::default();
        idx.update(
            "/app/Http/Controllers/UserController.php".into(),
            index_source("<?php namespace App\\Http\\Controllers; class UserController { public function show() {} public function __invoke() {} }"),
        );
        idx
    }

    #[test]
    fn gutter_markers_should_link_route_actions() {
        let src = "<?php\nuse App\\Http\\Controllers\\UserController;\nRoute::get('/u', [UserController::class, 'show']);\nRoute::get('/i', UserController::class);\n";
        let marks = gutter_markers(&index(), src, "routes/web.php");
        let found: Vec<(u32, &str)> = marks.iter().map(|m| (m.line, m.detail.as_str())).collect();
        assert_eq!(
            found,
            [(2, "UserController::show"), (3, "UserController::__invoke")]
        );
    }

    #[test]
    fn gutter_markers_should_run_commands() {
        let src = "<?php\nclass Send extends Command {\n    protected $signature = 'mail:send {user}';\n}\n";
        let marks = gutter_markers(&index(), src, "app/Console/Commands/Send.php");
        assert_eq!(
            marks[0].action,
            GutterAction::Run("php artisan mail:send".into())
        );
    }
}
