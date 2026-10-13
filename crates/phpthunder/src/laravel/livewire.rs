//! Livewire in Blade: `<livewire:name>` tags and their attributes, `wire:`
//! directives, and the values `wire:model` / `wire:click` take (the public
//! properties and methods of the component whose view is open).

use super::*;
use crate::analysis::Candidate;
use crate::symbol::Kind;
use crate::syntax::{parse, text};

/// `wire:` directives, with their modifiers' common forms.
const WIRE_DIRECTIVES: &[&str] = &[
    "wire:click",
    "wire:submit",
    "wire:model",
    "wire:model.live",
    "wire:model.blur",
    "wire:model.change",
    "wire:model.live.debounce.500ms",
    "wire:loading",
    "wire:loading.remove",
    "wire:target",
    "wire:dirty",
    "wire:poll",
    "wire:init",
    "wire:key",
    "wire:navigate",
    "wire:current",
    "wire:confirm",
    "wire:ignore",
    "wire:transition",
    "wire:offline",
    "wire:stream",
    "wire:replace",
    "wire:show",
    "wire:text",
    "wire:cloak",
    "wire:keydown.enter",
    "wire:change",
];

/// Directives whose value is a method of the component.
const METHOD_DIRECTIVES: &[&str] = &[
    "wire:click",
    "wire:submit",
    "wire:init",
    "wire:poll",
    "wire:target",
    "wire:keydown",
    "wire:change",
    "wire:confirm",
];

/// Livewire's own lifecycle methods, never called from a template.
const LIFECYCLE: &[&str] = &[
    "mount",
    "boot",
    "booted",
    "hydrate",
    "dehydrate",
    "render",
    "rendering",
    "rendered",
    "exception",
];

/// A Blade completion item.
fn item(label: &str, kind: Kind, detail: &str, insert: String) -> Candidate {
    Candidate {
        label: label.to_string(),
        kind,
        detail: detail.to_string(),
        insert_text: insert,
        import: None,
    }
}

/// Public properties and public non-lifecycle methods of the PHP class in
/// `src` (a Livewire component).
fn component_members(src: &str) -> (Vec<String>, Vec<String>) {
    let tree = parse(src);
    let (mut props, mut methods) = (Vec::new(), Vec::new());
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        let mut c = node.walk();
        stack.extend(node.named_children(&mut c));
        let public = {
            let mut c = node.walk();
            node.children(&mut c)
                .any(|m| m.kind() == "visibility_modifier" && text(m, src) == "public")
        };
        if !public {
            continue;
        }
        match node.kind() {
            "property_declaration" => {
                let mut c = node.walk();
                for el in node
                    .named_children(&mut c)
                    .filter(|e| e.kind() == "property_element")
                {
                    if let Some(name) = el.named_child(0) {
                        props.push(text(name, src).trim_start_matches('$').to_string());
                    }
                }
            }
            "method_declaration" => {
                let Some(name) = node.child_by_field_name("name") else {
                    continue;
                };
                let name = text(name, src);
                if !name.starts_with("__")
                    && !LIFECYCLE.contains(&name)
                    && !name.starts_with("updat")
                {
                    methods.push(name.to_string());
                }
            }
            _ => {}
        }
    }
    props.sort();
    methods.sort();
    (props, methods)
}

/// The component file behind the Livewire view `view` (`livewire.users.table`
/// is the component `users.table`).
fn component_of_view<'p>(project: &'p LaravelProject, view: &str) -> Option<&'p LaravelKey> {
    let name = view.strip_prefix("livewire.").unwrap_or(view);
    project.find(KeyKind::Livewire, name)
}

/// `FooBar` -> `foo-bar`.
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

/// Livewire completion at `offset` in a Blade template whose view name is
/// `view`; `None` when no Livewire context applies.
pub(super) fn livewire_completions(
    project: &LaravelProject,
    src: &str,
    offset: usize,
    view: Option<&str>,
) -> Option<(usize, Vec<Candidate>)> {
    let before = src.get(..offset)?;
    let word_start = before
        .rfind(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')))
        .map_or(0, |i| i + 1);
    let word = &before[word_start..];
    // `<livewire:us|`: component names.
    if before[..word_start].ends_with('<') && word.starts_with("livewire:") {
        let items = project
            .keys(KeyKind::Livewire)
            .iter()
            .map(|k| item(&k.name, Kind::Class, &k.detail, k.name.clone()))
            .collect();
        return Some((word_start + "livewire:".len(), items));
    }
    // Inside an open tag only from here on.
    let tag_open = before.rfind('<')?;
    if before[tag_open..].contains('>') {
        return None;
    }
    // `wire:model="na|"`: a value naming a property or method.
    if let Some(value) = attribute_value(before) {
        let (directive, value_start) = value;
        let file = view.and_then(|v| component_of_view(project, v))?;
        let src = std::fs::read_to_string(&file.path).ok()?;
        let (props, methods) = component_members(&src);
        let base = directive.split('.').next().unwrap_or(directive);
        let items: Vec<Candidate> = if base == "wire:model" {
            props
                .iter()
                .map(|p| item(p, Kind::Property, "property", p.clone()))
                .collect()
        } else if METHOD_DIRECTIVES.contains(&base) {
            methods
                .iter()
                .map(|m| item(m, Kind::Method, "method", m.clone()))
                .collect()
        } else {
            return None;
        };
        return Some((value_start, items));
    }
    if !before[..word_start].ends_with(char::is_whitespace) {
        return None;
    }
    // `<livewire:users.table us|`: the component's public properties.
    let tag: String = before[tag_open + 1..]
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    if let Some(name) = tag.strip_prefix("livewire:") {
        let file = project.find(KeyKind::Livewire, name)?;
        let src = std::fs::read_to_string(&file.path).ok()?;
        let items = component_members(&src)
            .0
            .iter()
            .map(|p| {
                let attr = kebab(p);
                item(&attr, Kind::Property, "attribute", format!("{attr}=\"\""))
            })
            .collect();
        return Some((word_start, items));
    }
    // `<button wi|`: `wire:` directives.
    if word.starts_with('w') {
        let items = WIRE_DIRECTIVES
            .iter()
            .map(|d| item(d, Kind::Property, "Livewire", format!("{d}=\"\"")))
            .collect();
        return Some((word_start, items));
    }
    None
}

/// For text ending inside an attribute value (`wire:click="sa`), the
/// attribute name and where the value starts.
fn attribute_value(before: &str) -> Option<(&str, usize)> {
    let quote_at = before.rfind('"')?;
    // An odd number of quotes since the tag opened means one is open.
    let tag_open = before.rfind('<')?;
    if before[tag_open..].matches('"').count().is_multiple_of(2) {
        return None;
    }
    let name_part = before[..quote_at].strip_suffix('=')?;
    let name_start = name_part
        .rfind(|c: char| c.is_whitespace())
        .map_or(0, |i| i + 1);
    let name = &name_part[name_start..];
    name.starts_with("wire:").then_some((name, quote_at + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_members_should_list_public_api() {
        let src = "<?php class Table extends Component { public $search = ''; public int $page; protected $x; public function mount() {} public function save() {} private function hide() {} public function render() {} }";
        let (props, methods) = component_members(src);
        assert_eq!(props, ["page", "search"]);
        assert_eq!(methods, ["save"]);
    }

    #[test]
    fn attribute_value_should_find_wire_attributes() {
        assert_eq!(
            attribute_value("<button wire:click=\"sa"),
            Some(("wire:click", 20))
        );
        assert_eq!(attribute_value("<a href=\"x"), None);
    }

    #[test]
    fn livewire_completions_should_offer_wire_directives_in_tags() {
        let project = LaravelProject::default();
        let src = "<button wi";
        let (start, items) = livewire_completions(&project, src, src.len(), None).unwrap();
        assert_eq!(start, 8);
        assert!(items.iter().any(|c| c.label == "wire:click"));
    }
}
