//! Blade completion: component tags (`<x-alert>`), their attributes,
//! directives (`@foreach`), and the variables a view can use (passed by
//! `view('name', [...])` or `compact()`, declared with `@props`, bound by
//! `@foreach`).

use super::*;
use crate::analysis::Candidate;
use crate::symbol::Kind;
use crate::syntax::{parse, text};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use tree_sitter::Node;

/// Directives offered after `@`, with what completing them inserts (`(`
/// asks the editor for parentheses with the caret inside).
const DIRECTIVES: &[(&str, &str)] = &[
    ("if", "if("),
    ("elseif", "elseif("),
    ("else", "else"),
    ("endif", "endif"),
    ("unless", "unless("),
    ("endunless", "endunless"),
    ("isset", "isset("),
    ("endisset", "endisset"),
    ("empty", "empty("),
    ("endempty", "endempty"),
    ("foreach", "foreach("),
    ("endforeach", "endforeach"),
    ("forelse", "forelse("),
    ("endforelse", "endforelse"),
    ("for", "for("),
    ("endfor", "endfor"),
    ("while", "while("),
    ("endwhile", "endwhile"),
    ("switch", "switch("),
    ("case", "case("),
    ("break", "break"),
    ("default", "default"),
    ("endswitch", "endswitch"),
    ("auth", "auth"),
    ("endauth", "endauth"),
    ("guest", "guest"),
    ("endguest", "endguest"),
    ("can", "can("),
    ("endcan", "endcan"),
    ("cannot", "cannot("),
    ("endcannot", "endcannot"),
    ("env", "env("),
    ("endenv", "endenv"),
    ("production", "production"),
    ("endproduction", "endproduction"),
    ("include", "include("),
    ("includeIf", "includeIf("),
    ("includeWhen", "includeWhen("),
    ("each", "each("),
    ("extends", "extends("),
    ("section", "section("),
    ("endsection", "endsection"),
    ("show", "show"),
    ("yield", "yield("),
    ("parent", "parent"),
    ("push", "push("),
    ("endpush", "endpush"),
    ("prepend", "prepend("),
    ("endprepend", "endprepend"),
    ("stack", "stack("),
    ("once", "once"),
    ("endonce", "endonce"),
    ("props", "props(["),
    ("aware", "aware(["),
    ("csrf", "csrf"),
    ("method", "method("),
    ("error", "error("),
    ("enderror", "enderror"),
    ("json", "json("),
    ("class", "class(["),
    ("style", "style(["),
    ("checked", "checked("),
    ("selected", "selected("),
    ("disabled", "disabled("),
    ("readonly", "readonly("),
    ("required", "required("),
    ("php", "php"),
    ("endphp", "endphp"),
    ("verbatim", "verbatim"),
    ("endverbatim", "endverbatim"),
    ("vite", "vite("),
    ("livewire", "livewire("),
    ("livewireStyles", "livewireStyles"),
    ("livewireScripts", "livewireScripts"),
    ("lang", "lang("),
    ("dd", "dd("),
    ("dump", "dump("),
];

/// Variables every view or component has.
const VIEW_GLOBALS: &[&str] = &[
    "$errors",
    "$loop",
    "$slot",
    "$attributes",
    "$component",
    "$app",
];

/// `FooBar` -> `foo-bar` (component names and attributes).
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

/// String content of a string literal node.
fn literal(node: Node, src: &str) -> Option<String> {
    matches!(node.kind(), "string" | "encapsed_string").then(|| {
        text(node, src)
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string()
    })
}

/// Keys of an array literal: string keys, or string values of a list
/// (`['type' => 'info', 'message']` gives `type`, `message`).
fn array_keys(array: Node, src: &str) -> Vec<String> {
    let mut c = array.walk();
    array
        .named_children(&mut c)
        .filter(|e| e.kind() == "array_element_initializer")
        .filter_map(|e| match (e.named_child(0), e.named_child(1)) {
            (Some(k), Some(_)) => literal(k, src),
            (Some(v), None) => literal(v, src),
            _ => None,
        })
        .collect()
}

/// The `@props([...])` keys of a Blade template.
pub fn blade_props(src: &str) -> Vec<String> {
    let Some(at) = src.find("@props(") else {
        return Vec::new();
    };
    let rest = &src[at + "@props(".len()..];
    let mut depth = 1usize;
    let mut end = rest.len();
    for (i, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    let php = format!("<?php {};", &rest[..end]);
    let tree = parse(&php);
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.kind() == "array_creation_expression" {
            return array_keys(n, &php).into_iter().map(|k| kebab(&k)).collect();
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    Vec::new()
}

/// Anonymous components under `resources/views/components` and class
/// components under `app/View/Components`, with their attributes.
pub fn blade_components(root: &Path) -> Vec<BladeComponent> {
    let mut out = Vec::new();
    let views = root.join("resources/views/components");
    for path in files_under(&views, ".blade.php") {
        let Ok(rel) = path.strip_prefix(&views) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let mut name = rel.trim_end_matches(".blade.php").replace('/', ".");
        if let Some(folder) = name.strip_suffix(".index") {
            name = folder.to_string();
        }
        let props = std::fs::read_to_string(&path)
            .map(|s| blade_props(&s))
            .unwrap_or_default();
        out.push(BladeComponent { name, path, props });
    }
    let classes = root.join("app/View/Components");
    for path in files_under(&classes, ".php") {
        let Ok(rel) = path.strip_prefix(&classes) else {
            continue;
        };
        let name = rel
            .to_string_lossy()
            .trim_end_matches(".php")
            .split(['/', '\\'])
            .map(kebab)
            .collect::<Vec<_>>()
            .join(".");
        let props = std::fs::read_to_string(&path)
            .map(|s| constructor_params(&s))
            .unwrap_or_default();
        out.push(BladeComponent { name, path, props });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// Constructor parameter names of the first class in `src`, kebab-cased.
fn constructor_params(src: &str) -> Vec<String> {
    let tree = parse(src);
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.kind() == "method_declaration"
            && n.child_by_field_name("name")
                .is_some_and(|m| text(m, src).eq_ignore_ascii_case("__construct"))
        {
            let Some(params) = n.child_by_field_name("parameters") else {
                return Vec::new();
            };
            let mut c = params.walk();
            return params
                .named_children(&mut c)
                .filter_map(|p| p.child_by_field_name("name"))
                .map(|v| kebab(text(v, src).trim_start_matches('$')))
                .collect();
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    Vec::new()
}

/// Variables passed to views across the project's PHP code under `app`
/// and `routes`: `view('name', [...])`, `View::make('name', compact(...))`
/// and `->view('name', [...])`, plus `->with('key', …)` / `->with([...])`
/// chained on them.
pub fn view_variables(root: &Path) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, BTreeSet<String>> = HashMap::new();
    for dir in ["app", "routes", "Modules"] {
        for path in files_under(&root.join(dir), ".php") {
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !src.contains("view") && !src.contains("View::") {
                continue;
            }
            collect_view_variables(&src, &mut out);
        }
    }
    out.into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect()
}

/// Adds the view variables passed in `src` to `out`.
fn collect_view_variables(src: &str, out: &mut HashMap<String, BTreeSet<String>>) {
    let tree = parse(src);
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
        let is_view_call = match n.kind() {
            "function_call_expression" => n
                .child_by_field_name("function")
                .is_some_and(|f| text(f, src).trim_start_matches('\\') == "view"),
            "scoped_call_expression" => {
                n.child_by_field_name("scope")
                    .is_some_and(|s| text(s, src).ends_with("View"))
                    && n.child_by_field_name("name")
                        .is_some_and(|m| text(m, src) == "make")
            }
            "member_call_expression" => n
                .child_by_field_name("name")
                .is_some_and(|m| matches!(text(m, src), "view" | "markdown")),
            _ => false,
        };
        if !is_view_call {
            continue;
        }
        let Some(args) = n.child_by_field_name("arguments") else {
            continue;
        };
        let mut ac = args.walk();
        let args: Vec<Node> = args
            .named_children(&mut ac)
            .filter_map(|a| a.named_child(0))
            .collect();
        let Some(name) = args.first().and_then(|a| literal(*a, src)) else {
            continue;
        };
        let vars = out.entry(name).or_default();
        if let Some(data) = args.get(1) {
            add_data(*data, src, vars);
        }
        // `->with(...)` chained on the call.
        let mut parent = n.parent();
        while let Some(p) = parent.filter(|p| p.kind() == "member_call_expression") {
            if p.child_by_field_name("name")
                .is_some_and(|m| text(m, src) == "with")
                && let Some(args) = p.child_by_field_name("arguments")
            {
                let mut wc = args.walk();
                let with: Vec<Node> = args
                    .named_children(&mut wc)
                    .filter_map(|a| a.named_child(0))
                    .collect();
                match with.as_slice() {
                    [key, _] => vars.extend(literal(*key, src)),
                    [data] => add_data(*data, src, vars),
                    _ => {}
                }
            }
            parent = p.parent();
        }
    }
}

/// Adds the names of a view-data argument: an array's keys or the
/// strings of `compact(...)`.
fn add_data(data: Node, src: &str, vars: &mut BTreeSet<String>) {
    match data.kind() {
        "array_creation_expression" => vars.extend(array_keys(data, src)),
        "function_call_expression"
            if data
                .child_by_field_name("function")
                .is_some_and(|f| text(f, src) == "compact") =>
        {
            if let Some(args) = data.child_by_field_name("arguments") {
                let mut c = args.walk();
                vars.extend(
                    args.named_children(&mut c)
                        .filter_map(|a| a.named_child(0))
                        .filter_map(|a| literal(a, src)),
                );
            }
        }
        _ => {}
    }
}

/// A proposal with a kind and detail.
fn candidate(label: String, kind: Kind, detail: &str, insert: String) -> Candidate {
    Candidate {
        label,
        kind,
        detail: detail.to_string(),
        insert_text: insert,
        import: None,
    }
}

/// Completion in a Blade template at byte `offset`: where the replaced
/// text starts and the proposals (unfiltered; the caller matches them
/// against the typed prefix). `view` is the template's view name
/// (`users.index`), for the variables passed to it. `None` outside
/// component tags, directives and variables.
pub fn blade_completions(
    project: &LaravelProject,
    src: &str,
    offset: usize,
    view: Option<&str>,
) -> Option<(usize, Vec<Candidate>)> {
    if let Some(found) = livewire::livewire_completions(project, src, offset, view) {
        return Some(found);
    }
    let before = src.get(..offset)?;
    let word_start = before
        .rfind(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')))
        .map_or(0, |i| i + 1);
    let lead = before[..word_start].chars().next_back();
    // `<x-forms.in|`: component names.
    if before[..word_start].ends_with('<') && before[word_start..].starts_with("x-") {
        let start = word_start + 2;
        let items = project
            .components
            .iter()
            .map(|c| {
                candidate(
                    c.name.clone(),
                    Kind::Class,
                    &c.path.to_string_lossy(),
                    c.name.clone(),
                )
            })
            .collect();
        return Some((start, items));
    }
    // `@for|`: directives.
    if lead == Some('@')
        && before[..word_start - 1]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric())
    {
        let items = DIRECTIVES
            .iter()
            .map(|(name, insert)| {
                candidate(
                    (*name).to_string(),
                    Kind::Function,
                    "directive",
                    (*insert).to_string(),
                )
            })
            .collect();
        return Some((word_start, items));
    }
    // `$us|`: variables of the view.
    if lead == Some('$') {
        let mut names: BTreeSet<String> = VIEW_GLOBALS
            .iter()
            .map(|s| s.trim_start_matches('$').to_string())
            .collect();
        if let Some(vars) = view.and_then(|v| project.view_variables.get(v)) {
            names.extend(vars.iter().cloned());
        }
        names.extend(blade_props(src).iter().map(|p| p.replace('-', "_")));
        // `@foreach ($users as $user)` and `@php $x = …` bind more names.
        let mut rest = src;
        while let Some(i) = rest.find('$') {
            let tail = &rest[i + 1..];
            let end = tail
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(tail.len());
            if end > 0 && (i + 1 + end) != (offset - (src.len() - rest.len())) {
                names.insert(tail[..end].to_string());
            }
            rest = &tail[end..];
        }
        let start = word_start - 1;
        let typed = &before[word_start..];
        let items = names
            .into_iter()
            .filter(|n| n != typed)
            .map(|n| candidate(format!("${n}"), Kind::Property, "variable", format!("${n}")))
            .collect();
        return Some((start, items));
    }
    // `<x-alert ty|`: attributes of the open component tag.
    let tag_start = before.rfind("<x-")?;
    if before[tag_start..].contains('>')
        || !before[word_start..].is_empty() && word_start == tag_start + 1
    {
        return None;
    }
    let tag: String = before[tag_start + 3..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '.' | ':'))
        .collect();
    if before.len() <= tag_start + 3 + tag.len()
        || !before[..word_start].ends_with(char::is_whitespace)
            && !before[..word_start].ends_with(':')
    {
        return None;
    }
    let component = project.components.iter().find(|c| c.name == tag)?;
    let items = component
        .props
        .iter()
        .map(|p| candidate(p.clone(), Kind::Property, "attribute", format!("{p}=\"\"")))
        .collect();
    Some((word_start, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> LaravelProject {
        let mut p = LaravelProject::default();
        p.components.push(BladeComponent {
            name: "forms.input".into(),
            path: "/p/input.blade.php".into(),
            props: vec!["type".into(), "label".into()],
        });
        p.view_variables
            .insert("users.index".into(), vec!["users".into(), "title".into()]);
        p
    }

    fn labels(result: Option<(usize, Vec<Candidate>)>) -> Vec<String> {
        result
            .map(|(_, items)| items.into_iter().map(|c| c.label).collect())
            .unwrap_or_default()
    }

    #[test]
    fn blade_completions_should_offer_components_attributes_directives_and_variables() {
        let p = project();
        let src = "<x-for";
        let (start, items) = blade_completions(&p, src, src.len(), None).unwrap();
        assert_eq!((start, items[0].label.as_str()), (3, "forms.input"));
        let src = "<x-forms.input ty";
        assert_eq!(
            labels(blade_completions(&p, src, src.len(), None)),
            ["type", "label"]
        );
        let src = "<div>@fore";
        assert!(
            labels(blade_completions(&p, src, src.len(), None)).contains(&"foreach".to_string())
        );
        let src = "@foreach ($users as $user)\n{{ $u";
        let vars = labels(blade_completions(&p, src, src.len(), Some("users.index")));
        assert!(
            vars.contains(&"$users".to_string())
                && vars.contains(&"$user".to_string())
                && vars.contains(&"$title".to_string()),
            "{vars:?}"
        );
        assert!(blade_completions(&p, "plain text", 5, None).is_none());
        assert!(blade_completions(&p, "mail@exa", 8, None).is_none());
    }

    #[test]
    fn props_and_view_data_should_be_read_from_source() {
        assert_eq!(
            blade_props("@props(['type' => 'info', 'message', 'isBig' => false])\n<div>"),
            ["type", "message", "is-big"]
        );
        let mut vars = HashMap::new();
        collect_view_variables(
            "<?php\nreturn view('users.index', ['users' => $u])->with('title', 'x');\nView::make('home', compact('a', 'b'));\n",
            &mut vars,
        );
        assert_eq!(
            vars["users.index"].iter().cloned().collect::<Vec<_>>(),
            ["title", "users"]
        );
        assert_eq!(vars["home"].iter().cloned().collect::<Vec<_>>(), ["a", "b"]);
    }
}
