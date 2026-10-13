//! Alpine.js in markup (Blade, HTML, Vue): `x-` directives, `@event` /
//! `x-on:` events with their modifiers, `:attr` / `x-bind:` bindings, and
//! inside attribute values and `<script>` blocks, JavaScript completion
//! with Alpine's `$magics` and the keys of the enclosing `x-data` objects.

use super::javascript::javascript_completions;
use crate::analysis::Candidate;
use crate::symbol::Kind;

/// Alpine directives.
const DIRECTIVES: &[&str] = &[
    "x-data",
    "x-init",
    "x-show",
    "x-bind",
    "x-on",
    "x-text",
    "x-html",
    "x-model",
    "x-modelable",
    "x-for",
    "x-transition",
    "x-effect",
    "x-ignore",
    "x-ref",
    "x-cloak",
    "x-teleport",
    "x-if",
    "x-id",
    "x-intersect",
    "x-collapse",
    "x-trap",
    "x-mask",
    "x-anchor",
    "x-sort",
];

/// DOM events offered after `@` / `x-on:`.
const EVENTS: &[&str] = &[
    "click",
    "dblclick",
    "submit",
    "input",
    "change",
    "keydown",
    "keyup",
    "keypress",
    "focus",
    "blur",
    "mouseenter",
    "mouseleave",
    "mouseover",
    "mouseout",
    "mousedown",
    "mouseup",
    "scroll",
    "resize",
    "load",
    "contextmenu",
    "wheel",
    "drag",
    "dragstart",
    "dragend",
    "dragover",
    "drop",
    "touchstart",
    "touchend",
    "paste",
    "copy",
];

/// Event modifiers, including key names.
const MODIFIERS: &[&str] = &[
    "prevent", "stop", "outside", "window", "document", "once", "debounce", "throttle", "self",
    "camel", "dot", "passive", "capture", "enter", "escape", "space", "tab", "shift", "ctrl",
    "alt", "meta", "cmd", "up", "down", "left", "right",
];

/// Attributes commonly bound with `:`.
const BINDINGS: &[&str] = &[
    "class",
    "style",
    "disabled",
    "href",
    "src",
    "value",
    "id",
    "checked",
    "selected",
    "hidden",
    "title",
    "placeholder",
    "aria-expanded",
    "aria-hidden",
    "key",
    "readonly",
    "required",
    "type",
    "name",
    "for",
];

/// Alpine's magic properties.
const MAGICS: &[(&str, &str)] = &[
    ("$el", "current element"),
    ("$refs", "x-ref elements"),
    ("$store", "global stores"),
    ("$watch", "watch a property"),
    ("$dispatch", "dispatch an event"),
    ("$nextTick", "after the next update"),
    ("$root", "x-data root element"),
    ("$data", "component data"),
    ("$id", "unique id"),
    ("$event", "the event (in handlers)"),
    ("$wire", "Livewire component"),
];

/// A completion item inserting `insert`.
fn item(label: &str, kind: Kind, detail: &str, insert: String) -> Candidate {
    Candidate {
        label: label.to_string(),
        kind,
        detail: detail.to_string(),
        insert_text: insert,
        import: None,
    }
}

/// Whether an attribute holds JavaScript (see the IDE highlighter).
fn is_script_attribute(name: &str) -> bool {
    !matches!(
        name,
        "x-ref" | "x-cloak" | "x-ignore" | "x-teleport" | "x-id"
    ) && !name.starts_with("x-transition")
        && (name.starts_with("x-") || name.starts_with('@') || name.starts_with(':'))
}

/// Alpine completion at `offset` in a markup file; `None` outside Alpine
/// contexts (the caller falls back to its own completion).
pub fn alpine_completions(src: &str, offset: usize) -> Option<(usize, Vec<Candidate>)> {
    let before = src.get(..offset)?;
    // `<script>` blocks are JavaScript.
    if let Some(open) = before.rfind("<script")
        && !before[open..].contains("</script")
        && let Some(gt) = before[open..].find('>')
    {
        let code_start = open + gt + 1;
        let (start, items) = javascript_completions(
            &src[code_start..script_end(src, code_start)],
            offset - code_start,
            &magics(src, offset),
        );
        return Some((code_start + start, items));
    }
    let tag_open = before.rfind('<')?;
    if before[tag_open..].contains('>') {
        return None;
    }
    // Inside a quoted attribute value.
    if let Some((name, value_start)) = attribute_value(before, tag_open) {
        if !is_script_attribute(name) {
            return None;
        }
        let value_end = src[value_start..]
            .find('"')
            .map_or(src.len(), |i| value_start + i);
        let (start, items) = javascript_completions(
            &src[value_start..value_end.max(offset)],
            offset - value_start,
            &magics(src, offset),
        );
        return Some((value_start + start, items));
    }
    let word_start = before
        .rfind(|c: char| c.is_whitespace())
        .map_or(0, |i| i + 1)
        .max(tag_open + 1);
    let word = &before[word_start..];
    if word_start == tag_open + 1 {
        return None;
    }
    // `@click.prev|` / `x-on:keydown.ent|`: modifiers.
    if (word.starts_with('@') || word.starts_with("x-on:")) && word.contains('.') {
        let dot = word_start + word.rfind('.')? + 1;
        let items = MODIFIERS
            .iter()
            .map(|m| item(m, Kind::Constant, "modifier", (*m).to_string()))
            .collect();
        return Some((dot, items));
    }
    // `@cl|` / `x-on:cl|`: events.
    if let Some(prefix) = ["@", "x-on:"].iter().find(|p| word.starts_with(**p)) {
        let items = EVENTS
            .iter()
            .map(|e| item(e, Kind::Function, "event", format!("{e}=\"\"")))
            .collect();
        return Some((word_start + prefix.len(), items));
    }
    // `:cla|` / `x-bind:cla|`: bindings.
    if let Some(prefix) = [":", "x-bind:"].iter().find(|p| word.starts_with(**p)) {
        let items = BINDINGS
            .iter()
            .map(|a| item(a, Kind::Property, "binding", format!("{a}=\"\"")))
            .collect();
        return Some((word_start + prefix.len(), items));
    }
    // `x-da|`: directives.
    if word.starts_with('x') {
        let items = DIRECTIVES
            .iter()
            .map(|d| {
                let insert = if matches!(*d, "x-cloak" | "x-ignore" | "x-collapse" | "x-transition")
                {
                    (*d).to_string()
                } else {
                    format!("{d}=\"\"")
                };
                item(d, Kind::Property, "Alpine", insert)
            })
            .collect();
        return Some((word_start, items));
    }
    None
}

/// Where the `<script>` block whose code starts at `from` ends.
fn script_end(src: &str, from: usize) -> usize {
    src[from..].find("</script").map_or(src.len(), |i| from + i)
}

/// For text ending inside a double-quoted attribute value of the tag opened
/// at `tag_open`: the attribute name and where the value starts.
fn attribute_value(before: &str, tag_open: usize) -> Option<(&str, usize)> {
    if before[tag_open..].matches('"').count().is_multiple_of(2) {
        return None;
    }
    let quote_at = before.rfind('"')?;
    let name_part = before[..quote_at].strip_suffix('=')?;
    let name_start = name_part
        .rfind(|c: char| c.is_whitespace())
        .map_or(0, |i| i + 1);
    Some((&name_part[name_start..], quote_at + 1))
}

/// Alpine's magics plus the keys of every `x-data` object before `offset`.
fn magics(src: &str, offset: usize) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = MAGICS
        .iter()
        .map(|(m, d)| ((*m).to_string(), (*d).to_string()))
        .collect();
    let before = &src[..offset];
    let mut from = 0;
    while let Some(i) = before[from..].find("x-data=\"") {
        let start = from + i + "x-data=\"".len();
        let end = src[start..].find('"').map_or(src.len(), |n| start + n);
        for key in object_keys(&src[start..end]) {
            if !out.iter().any(|(k, _)| *k == key) {
                out.push((key, "x-data".to_string()));
            }
        }
        from = start;
    }
    out
}

/// Top-level keys of a JavaScript object literal (`{ open: false,
/// toggle() {…} }` gives `open`, `toggle`).
fn object_keys(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut word = String::new();
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' | '`' => quote = Some(c),
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            _ => {}
        }
        if depth == 1 && (c.is_alphanumeric() || c == '_' || c == '$') {
            word.push(c);
        } else {
            if !word.is_empty() && depth >= 1 {
                let next = chars[i..].iter().find(|c| !c.is_whitespace());
                if matches!(next, Some(':' | '('))
                    && !word.starts_with(|c: char| c.is_ascii_digit())
                {
                    out.push(word.clone());
                }
            }
            word.clear();
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(src: &str) -> Vec<String> {
        let offset = src.find('|').unwrap();
        let src = src.replacen('|', "", 1);
        alpine_completions(&src, offset)
            .map(|(_, items)| items.into_iter().map(|c| c.label).collect())
            .unwrap_or_default()
    }

    #[test]
    fn alpine_completions_should_offer_directives_events_and_modifiers() {
        assert!(labels("<div x-|").contains(&"x-data".to_string()));
        assert!(labels("<button @cl|").contains(&"click".to_string()));
        assert!(labels("<form @submit.pre|").contains(&"prevent".to_string()));
    }

    #[test]
    fn alpine_completions_should_offer_data_keys_and_magics_in_values() {
        let got = labels(
            "<div x-data=\"{ open: false, toggle() { this.open = !this.open } }\"><button @click=\"to|\"></button></div>",
        );
        assert!(
            got.contains(&"toggle".to_string())
                && got.contains(&"open".to_string())
                && got.contains(&"$dispatch".to_string())
        );
    }

    #[test]
    fn alpine_completions_should_ignore_plain_attributes() {
        assert!(labels("<a href=\"ho|\">").is_empty());
    }

    #[test]
    fn object_keys_should_read_top_level_only() {
        assert_eq!(
            object_keys("{ a: 1, b: { c: 2 }, d() {} }"),
            ["a", "b", "d"]
        );
    }
}
