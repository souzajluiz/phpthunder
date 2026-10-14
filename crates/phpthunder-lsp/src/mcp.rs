//! Model Context Protocol server (`phpthunder --mcp [project]`): the
//! engine's project index offered to AI agents as tools over stdio
//! (newline-delimited JSON-RPC 2.0): symbol search, declarations, usages,
//! inspections, type hierarchy, class diagrams and file outlines.

use parking_lot::RwLock;
use phpthunder::analysis::{Definition, find_references};
use phpthunder::index::{self, Index, index_source};
use phpthunder::symbol::{Kind, MemberTag, Symbol};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Protocol revision this server speaks.
const PROTOCOL: &str = "2025-06-18";

/// The indexed project the tools answer about.
struct Project {
    /// Project folder; relative paths in tool arguments start here.
    root: PathBuf,
    /// Declarations of every PHP file (vendor included) and the built-ins.
    index: RwLock<Index>,
}

impl Project {
    /// Indexes every PHP file under `root` plus PHP's built-ins.
    fn open(root: PathBuf) -> Self {
        let index = RwLock::new(Index::default());
        if let Ok(path) = index::write_builtins() {
            index
                .write()
                .update(path, index_source(index::BUILTINS_SOURCE));
        }
        index::index_files(&index, &index::php_files(&root), |_, _| {});
        Self { root, index }
    }

    /// `path` as given, or relative to the project.
    fn resolve(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }

    /// `path` relative to the project, for answers.
    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    /// Declarations named `name`: a class-like or function FQN, or
    /// `Class::member` (method, property, constant).
    fn definitions(&self, name: &str) -> Vec<Definition> {
        let index = self.index.read();
        let name = name.trim_start_matches('\\');
        let found: Vec<(&Path, &Symbol)> = match name.split_once("::") {
            Some((class, member)) => {
                let member = member.trim_start_matches('$').trim_end_matches("()");
                [MemberTag::Method, MemberTag::Property, MemberTag::Constant]
                    .into_iter()
                    .flat_map(|tag| index.find_member(class, tag, member))
                    .collect()
            }
            None => {
                let classes = index.classes(name);
                if classes.is_empty() {
                    index.functions(&[name.to_string()])
                } else {
                    classes
                }
            }
        };
        found
            .into_iter()
            .map(|(path, symbol)| Definition {
                path: path.to_path_buf(),
                symbol: symbol.clone(),
            })
            .collect()
    }

    /// One line describing `symbol` declared in `path`.
    fn describe(&self, path: &Path, symbol: &Symbol) -> String {
        let owner = if symbol.kind.is_member() {
            format!("{}::", symbol.fqn)
        } else {
            String::new()
        };
        let name = if symbol.kind.is_member() {
            &symbol.name
        } else {
            &symbol.fqn
        };
        format!(
            "{:?} {owner}{name} — {}:{}",
            symbol.kind,
            self.relative(path),
            symbol.name_span.start.line + 1
        )
    }
}

/// The tools offered, as MCP tool descriptions.
fn tools() -> Value {
    let text = |name: &str, description: &str| json!({ name: { "type": "string", "description": description } });
    let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({
            "name": name,
            "description": description,
            "inputSchema": { "type": "object", "properties": properties, "required": required },
        })
    };
    json!([
        tool(
            "search_symbols",
            "Find PHP classes, interfaces, traits, enums, functions and members whose name contains the query.",
            text("query", "Part of a name, case-insensitive."),
            &["query"],
        ),
        tool(
            "symbol_info",
            "Declaration of a PHP symbol: file, line, signature and doc comment.",
            text(
                "name",
                "Fully qualified class or function name, or Class::member."
            ),
            &["name"],
        ),
        tool(
            "find_usages",
            "Every place in the project that uses a PHP symbol.",
            text(
                "name",
                "Fully qualified class or function name, or Class::member."
            ),
            &["name"],
        ),
        tool(
            "file_diagnostics",
            "Problems the engine's inspections find in a PHP file.",
            text("path", "File path, absolute or relative to the project."),
            &["path"],
        ),
        tool(
            "type_hierarchy",
            "Supertypes (parents, interfaces, traits) and subtypes of a class.",
            text("class", "Fully qualified class name."),
            &["class"],
        ),
        tool(
            "class_diagram",
            "PlantUML class diagram of a class with its supertypes and subtypes.",
            text("class", "Fully qualified class name."),
            &["class"],
        ),
        tool(
            "file_outline",
            "Classes, functions and members declared in a PHP file, with lines.",
            text("path", "File path, absolute or relative to the project."),
            &["path"],
        ),
    ])
}

/// Runs tool `name` with `args`; the text answer or an error message.
fn call(project: &Project, name: &str, args: &Value) -> Result<String, String> {
    let arg = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("missing argument `{key}`"))
    };
    match name {
        "search_symbols" => {
            let query = arg("query")?;
            let index = project.index.read();
            let lines: Vec<String> = index
                .search(&query, 50)
                .into_iter()
                .filter(|(_, s)| !s.is_virtual)
                .map(|(p, s)| project.describe(p, s))
                .collect();
            Ok(if lines.is_empty() {
                "No symbols found.".into()
            } else {
                lines.join("\n")
            })
        }
        "symbol_info" => {
            let defs = project.definitions(&arg("name")?);
            if defs.is_empty() {
                return Err("No such symbol.".into());
            }
            Ok(defs
                .iter()
                .map(|d| {
                    let mut out = project.describe(&d.path, &d.symbol);
                    if !d.symbol.signature.is_empty() {
                        out.push_str(&format!("\n{}", d.symbol.signature));
                    }
                    if !d.symbol.doc.is_empty() {
                        out.push_str(&format!("\n{}", d.symbol.doc));
                    }
                    out
                })
                .collect::<Vec<_>>()
                .join("\n\n"))
        }
        "find_usages" => {
            let defs = project.definitions(&arg("name")?);
            if defs.is_empty() {
                return Err("No such symbol.".into());
            }
            let index = project.index.read();
            let usages = find_references(&index, &defs, |p| std::fs::read_to_string(p).ok());
            let lines: Vec<String> = usages
                .iter()
                .filter(|u| !u.is_declaration)
                .map(|u| format!("{}:{}", project.relative(&u.path), u.span.start.line + 1))
                .collect();
            Ok(format!("{} usages\n{}", lines.len(), lines.join("\n")))
        }
        "file_diagnostics" => {
            let path = project.resolve(&arg("path")?);
            let src = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let found = phpthunder::inspection::diagnostics(&project.index.read(), &src);
            Ok(if found.is_empty() {
                "No problems.".into()
            } else {
                found
                    .iter()
                    .map(|d| format!("{}: {:?}: {}", d.span.start.line + 1, d.severity, d.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
        "type_hierarchy" => {
            let class = arg("class")?;
            let index = project.index.read();
            let supers = index.hierarchy(&class);
            let subs = phpthunder::refactor::subtypes(&index, &class);
            Ok(format!(
                "Supertypes:\n{}\nSubtypes:\n{}",
                supers
                    .iter()
                    .skip(1)
                    .map(|s| format!("  {s}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                subs.iter()
                    .map(|(p, s)| format!("  {}", project.describe(p, s)))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
        }
        "class_diagram" => {
            let class = arg("class")?;
            let diagram = phpthunder::diagram::hierarchy_diagram(&project.index.read(), &class);
            Ok(phpthunder::diagram::to_plantuml(&diagram))
        }
        "file_outline" => {
            let path = project.resolve(&arg("path")?);
            let index = project.index.read();
            let symbols = index.file_symbols(&path);
            if symbols.is_empty() {
                return Err("File not indexed (not a PHP file of the project?).".into());
            }
            Ok(symbols
                .iter()
                .filter(|s| !s.is_virtual)
                .map(|s| {
                    let indent = if s.kind.is_member() { "  " } else { "" };
                    let label = if s.kind == Kind::Method {
                        format!("{}()", s.name)
                    } else if s.kind.is_member() {
                        s.name.clone()
                    } else {
                        s.fqn.clone()
                    };
                    format!(
                        "{indent}{:?} {label} :{}",
                        s.kind,
                        s.name_span.start.line + 1
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        _ => Err(format!("unknown tool `{name}`")),
    }
}

/// The response to one request, or `None` for notifications.
fn handle(project: &Project, request: &Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    let method = request["method"].as_str().unwrap_or_default();
    let result = match method {
        "initialize" => json!({
            "protocolVersion": request["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "phpthunder", "version": env!("CARGO_PKG_VERSION") },
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = request["params"]["name"].as_str().unwrap_or_default();
            let args = &request["params"]["arguments"];
            match call(project, name, args) {
                Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
                Err(error) => {
                    json!({ "content": [{ "type": "text", "text": error }], "isError": true })
                }
            }
        }
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("method not found: {method}") },
            }));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// Serves MCP on stdin/stdout for the project at `root` until stdin
/// closes. Indexes the project first.
///
/// # Errors
/// Returns the I/O error when stdout cannot be written.
pub fn serve(root: PathBuf) -> std::io::Result<()> {
    let project = Project::open(root);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle(&project, &request),
            Err(error) => Some(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": error.to_string() },
            })),
        };
        if let Some(response) = response {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project holding one small file, indexed in a temporary folder.
    fn project() -> Project {
        let root = std::env::temp_dir().join(format!("phpthunder-mcp-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("a.php"),
            "<?php\nnamespace App;\n/** A user. */\nclass User { public function save(): bool {} }\nclass Admin extends User {}\nfunction run() { (new User)->save(); }\n",
        )
        .unwrap();
        Project::open(root)
    }

    /// The text of calling `tool` with `args`.
    fn text(project: &Project, tool: &str, args: Value) -> String {
        let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": args}});
        handle(project, &request).unwrap()["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn tools_should_answer_about_the_project() {
        let p = project();
        assert!(text(&p, "search_symbols", json!({"query": "admin"})).contains("App\\Admin"));
        assert!(text(&p, "symbol_info", json!({"name": "App\\User"})).contains("A user."));
        assert!(
            text(&p, "find_usages", json!({"name": "App\\User::save"})).starts_with("1 usages")
        );
        assert!(text(&p, "type_hierarchy", json!({"class": "App\\User"})).contains("App\\Admin"));
        assert!(text(&p, "file_outline", json!({"path": "a.php"})).contains("save()"));
        let _ = std::fs::remove_dir_all(&p.root);
    }

    #[test]
    fn initialize_should_announce_tools() {
        let p = Project {
            root: PathBuf::new(),
            index: RwLock::new(Index::default()),
        };
        let reply = handle(
            &p,
            &json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}}),
        )
        .unwrap();
        assert_eq!(reply["result"]["serverInfo"]["name"], "phpthunder");
    }

    #[test]
    fn notifications_should_get_no_reply() {
        let p = Project {
            root: PathBuf::new(),
            index: RwLock::new(Index::default()),
        };
        assert!(
            handle(
                &p,
                &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
            )
            .is_none()
        );
    }
}
