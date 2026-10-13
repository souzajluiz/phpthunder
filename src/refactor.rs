//! Editing services built on resolution: parameter info while typing a
//! call, rename across the project, and import cleanup.

use crate::analysis::{Analyzer, Target, find_references, resolve};
use crate::diagnostics::{Code, Edit, diagnostics};
use crate::index::Index;
use crate::stubs::{Kind, MemberTag, Symbol};
use crate::syntax::{is_class_like, parse, text};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Parameter info for the call around the caret (`⌘P` / typing `(` or `,`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureHelp {
    /// Declaration header of the callee, e.g. `public function find(int $id): ?User`.
    pub label: String,
    /// Each parameter as written in the declaration.
    pub params: Vec<String>,
    /// Index of the parameter the caret is in (clamped for variadics).
    pub active: usize,
}

/// Splits `a, b<c, d>, e` at top-level commas (not inside brackets).
fn split_top_level(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        parts.push(last.to_string());
    }
    parts
}

/// Parameters listed in a declaration header (between its outer parentheses).
fn params_of(signature: &str) -> Vec<String> {
    let Some(open) = signature.find('(') else {
        return Vec::new();
    };
    let mut depth = 0i32;
    let mut close = signature.len();
    for (i, ch) in signature[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    split_top_level(&signature[open + 1..close])
}

/// The callee declaration for a call node, if it resolves.
fn callee(index: &Index, analyzer: &Analyzer, call: Node) -> Option<Symbol> {
    let name = match call.kind() {
        "function_call_expression" => {
            let f = call.child_by_field_name("function")?;
            let mut n = f;
            // `App\fn()`: classify the last segment.
            while n.kind() != "name" {
                n = n.named_child(n.named_child_count().checked_sub(1)? as u32)?;
            }
            n
        }
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            call.child_by_field_name("name")?
        }
        "object_creation_expression" => {
            let mut c = call.walk();
            let class = call
                .named_children(&mut c)
                .find(|n| matches!(n.kind(), "name" | "qualified_name"))?;
            let class_name = if class.kind() == "name" {
                class
            } else {
                class.named_child(class.named_child_count().checked_sub(1)? as u32)?
            };
            let Target::Class(fqn) = analyzer.target_of(class_name)? else {
                return None;
            };
            let ctor = index.find_member(&fqn, MemberTag::Method, "__construct");
            return ctor.first().map(|(_, s)| (*s).clone());
        }
        _ => return None,
    };
    let target = analyzer.target_of(name)?;
    resolve(index, &target).into_iter().next().map(|d| d.symbol)
}

/// Parameter info at `byte`, or `None` when the caret is not inside the
/// argument list of a resolvable call.
pub fn signature_help(index: &Index, src: &str, byte: usize) -> Option<SignatureHelp> {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    let mut node = tree.root_node().descendant_for_byte_range(byte, byte)?;
    // Walk up to the innermost argument list that contains the caret.
    let args = loop {
        if node.kind() == "arguments"
            && node.start_byte() < byte
            && (byte < node.end_byte() || node.has_error() || text(node, src).ends_with('('))
        {
            break node;
        }
        node = node.parent()?;
    };
    let call = args.parent()?;
    let symbol = callee(index, &analyzer, call)?;
    let params = params_of(&symbol.signature);
    let mut commas = 0;
    let mut c = args.walk();
    for child in args.children(&mut c) {
        if child.kind() == "," && child.end_byte() <= byte {
            commas += 1;
        }
    }
    let variadic = params.last().is_some_and(|p| p.contains("..."));
    let active = if variadic {
        commas.min(params.len().saturating_sub(1))
    } else {
        commas
    };
    Some(SignatureHelp {
        label: symbol.signature,
        params,
        active,
    })
}

/// Result of a rename: text edits per file, plus a file rename when a class
/// is renamed in a file named after it (PSR-4).
#[derive(Clone, Debug, Default)]
pub struct RenamePlan {
    /// Edits grouped by file, each sorted by offset.
    pub edits: BTreeMap<PathBuf, Vec<Edit>>,
    /// `(old, new)` path when the class file should be renamed too.
    pub file_rename: Option<(PathBuf, PathBuf)>,
}

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
        let lines = crate::text::LineIndex::new(&text);
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

/// "Optimize imports": removes unused `use` statements and sorts each
/// contiguous block of single-line imports alphabetically.
pub fn optimize_imports(index: &Index, src: &str) -> String {
    let unused: Vec<Edit> = diagnostics(index, src)
        .into_iter()
        .filter(|d| d.code == Code::UnusedImport)
        .flat_map(|d| crate::diagnostics::quick_fixes(index, src, &d, |_| None))
        .flat_map(|f| f.edits)
        .collect();
    let cleaned = apply_edits(src, &unused);
    sort_import_blocks(&cleaned)
}

/// Sorts runs of consecutive `use X;` lines (case-insensitive).
fn sort_import_blocks(src: &str) -> String {
    let mut lines: Vec<&str> = src.split('\n').collect();
    let is_use = |l: &str| {
        let t = l.trim_start();
        t.starts_with("use ") && t.ends_with(';') && !t.contains('{')
    };
    let mut i = 0;
    while i < lines.len() {
        if !is_use(lines[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && is_use(lines[i]) {
            i += 1;
        }
        let block = &mut lines[start..i];
        // `use function` and `use const` stay after class imports, as PSR-12 groups them.
        block.sort_by_key(|l| {
            let t = l.trim_start();
            let group = if t.starts_with("use function ") {
                1
            } else if t.starts_with("use const ") {
                2
            } else {
                0
            };
            (group, t.to_ascii_lowercase())
        });
    }
    lines.join("\n")
}

/// Members a class must still implement: abstract methods from its parent
/// chain and interface methods with no implementation in the hierarchy.
pub fn missing_implementations(index: &Index, class: &str) -> Vec<Symbol> {
    let mut missing = Vec::new();
    for cls in index.hierarchy(class).iter().skip(1) {
        let Some((path, decl)) = index.classes(cls).into_iter().next() else {
            continue;
        };
        for member in index
            .file_symbols(path)
            .iter()
            .filter(|s| s.kind == Kind::Method && s.fqn.eq_ignore_ascii_case(&decl.fqn))
        {
            let is_abstract = decl.kind == Kind::Interface
                || member.signature.split_whitespace().any(|w| w == "abstract");
            if !is_abstract {
                continue;
            }
            let implemented = index
                .find_member(class, MemberTag::Method, &member.name)
                .iter()
                .any(|(_, s)| {
                    let owner_kind = index.classes(&s.fqn).first().map(|(_, c)| c.kind);
                    owner_kind != Some(Kind::Interface)
                        && !s.signature.split_whitespace().any(|w| w == "abstract")
                });
            if !implemented
                && !missing
                    .iter()
                    .any(|m: &Symbol| m.name.eq_ignore_ascii_case(&member.name))
            {
                missing.push(member.clone());
            }
        }
    }
    missing
}

/// Source for implementing `method` (signature without `abstract`, empty body).
pub fn implementation_stub(method: &Symbol) -> String {
    let header: Vec<&str> = method
        .signature
        .split_whitespace()
        .filter(|w| *w != "abstract")
        .collect();
    let mut header = header.join(" ");
    if !header.starts_with("public")
        && !header.starts_with("protected")
        && !header.starts_with("private")
    {
        header = format!("public {header}");
    }
    format!(
        "\n    {header}\n    {{\n        throw new \\RuntimeException('Not implemented');\n    }}\n"
    )
}

/// Classes that extend or implement `class` (directly or not): "go to
/// implementation" and the type hierarchy's subtype side.
pub fn subtypes(index: &Index, class: &str) -> Vec<(PathBuf, Symbol)> {
    let mut out = Vec::new();
    for (path, symbols) in index.files() {
        for s in symbols
            .iter()
            .filter(|s| s.kind.is_class_like() && !s.fqn.eq_ignore_ascii_case(class))
        {
            if index
                .hierarchy(&s.fqn)
                .iter()
                .skip(1)
                .any(|c| c.eq_ignore_ascii_case(class))
            {
                out.push((path.to_path_buf(), s.clone()));
            }
        }
    }
    out.sort_by(|a, b| a.1.fqn.cmp(&b.1.fqn));
    out
}

/// Methods that override or implement `method` of `class` in subtypes.
pub fn overriding_methods(index: &Index, class: &str, method: &str) -> Vec<(PathBuf, Symbol)> {
    subtypes(index, class)
        .into_iter()
        .flat_map(|(path, cls)| {
            index
                .file_symbols(&path)
                .iter()
                .filter(|s| {
                    s.kind == Kind::Method
                        && s.fqn.eq_ignore_ascii_case(&cls.fqn)
                        && s.name.eq_ignore_ascii_case(method)
                })
                .map(|s| (path.clone(), s.clone()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Whether `node` is inside a class-like declaration (used by callers that
/// generate members at the caret).
pub fn enclosing_class_fqn(src: &str, byte: usize) -> Option<String> {
    let tree = parse(src);
    let mut node = tree.root_node().descendant_for_byte_range(byte, byte)?;
    loop {
        if is_class_like(node.kind()) {
            let name = node.child_by_field_name("name")?;
            let ctx = crate::syntax::context_at(tree.root_node(), src, byte);
            return Some(ctx.qualify(text(name, src)));
        }
        node = node.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::index_source;

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
