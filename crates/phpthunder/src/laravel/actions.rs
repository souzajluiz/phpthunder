//! Laravel context actions for the `⌥↵` menu: switching a Blade echo
//! between escaped `{{ }}` and raw `{!! !!}`, and a validation rule string
//! (`'required|email'`) to a rule array (`['required', 'email']`) and back.

use super::rules::in_rules_array;
use crate::inspection::{Edit, Fix, FixKind};

/// How far (bytes) to look around the caret for a Blade echo.
const ECHO_WINDOW: usize = 4000;

/// The context actions that apply at `offset`; `blade` for Blade templates.
pub fn laravel_actions(src: &str, offset: usize, blade: bool) -> Vec<Fix> {
    let mut out = Vec::new();
    if blade {
        out.extend(toggle_echo(src, offset));
    } else {
        out.extend(rules_to_array(src, offset));
        out.extend(rules_to_string(src, offset));
    }
    out
}

/// A whole-text edit in the current file.
fn edit(start: usize, end: usize, text: String) -> Edit {
    Edit {
        path: None,
        start,
        end,
        text,
    }
}

/// Switches the Blade echo around `offset` between `{{ … }}` and `{!! … !!}`.
fn toggle_echo(src: &str, offset: usize) -> Option<Fix> {
    let from = floor(src, offset.saturating_sub(ECHO_WINDOW));
    for (open, close, raw) in [("{!!", "!!}", true), ("{{", "}}", false)] {
        let Some(found) = src[from..floor(src, offset)].rfind(open) else {
            continue;
        };
        let start = from + found;
        // `{{--` opens a comment, not an echo.
        if !raw && src[start..].starts_with("{{--") {
            continue;
        }
        let Some(len) = src[start + open.len()..].find(close) else {
            continue;
        };
        let end = start + open.len() + len + close.len();
        if offset > end {
            continue;
        }
        let inner = &src[start + open.len()..end - close.len()];
        let (new_open, new_close) = if raw { ("{{", "}}") } else { ("{!!", "!!}") };
        return Some(Fix {
            title: if raw {
                "Escape output ({{ }})"
            } else {
                "Output unescaped ({!! !!})"
            }
            .into(),
            edits: vec![edit(start, end, format!("{new_open}{inner}{new_close}"))],
            kind: FixKind::ToggleBladeEcho { to_raw: !raw },
        });
    }
    None
}

/// Largest char boundary at or before `i`.
fn floor(src: &str, mut i: usize) -> usize {
    i = i.min(src.len());
    while !src.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The quoted string on the caret's line that contains `offset`: content
/// start, content end and the quote character.
fn string_around(src: &str, offset: usize) -> Option<(usize, usize, char)> {
    let line_start = src.get(..offset)?.rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[offset..].find('\n').map_or(src.len(), |i| offset + i);
    let mut open: Option<(usize, char)> = None;
    for (i, c) in src[line_start..line_end].char_indices() {
        let at = line_start + i;
        match open {
            None if c == '\'' || c == '"' => open = Some((at, c)),
            Some((start, q)) if c == q && !src[..at].ends_with('\\') => {
                if (start + 1..=at).contains(&offset) {
                    return Some((start + 1, at, q));
                }
                open = None;
            }
            _ => {}
        }
    }
    None
}

/// `'required|email'` in a rules array -> `['required', 'email']`.
fn rules_to_array(src: &str, offset: usize) -> Option<Fix> {
    let (start, end, quote) = string_around(src, offset)?;
    let content = &src[start..end];
    if !content.contains('|') || !in_rules_array(src, start) {
        return None;
    }
    let items: Vec<String> = content
        .split('|')
        .map(|r| format!("{quote}{}{quote}", r.trim()))
        .collect();
    Some(Fix {
        title: "Convert rules to an array".into(),
        edits: vec![edit(start - 1, end + 1, format!("[{}]", items.join(", ")))],
        kind: FixKind::RulesToArray,
    })
}

/// `['required', 'email']` (plain strings only) in a rules array ->
/// `'required|email'`.
fn rules_to_string(src: &str, offset: usize) -> Option<Fix> {
    let line_start = src.get(..offset)?.rfind('\n').map_or(0, |i| i + 1);
    let open = line_start + src[line_start..offset].rfind('[')?;
    let close = open + src[open..].find(']')?;
    if offset > close + 1 || !in_rules_array(src, open) {
        return None;
    }
    let inner = src[open + 1..close].trim().trim_end_matches(',');
    let mut rules = Vec::new();
    let mut quote = '\'';
    for item in inner.split(',') {
        let item = item.trim();
        let q = item.chars().next().filter(|c| *c == '\'' || *c == '"')?;
        let body = item.strip_prefix(q)?.strip_suffix(q)?;
        if body.contains(['|', '\'', '"']) {
            return None;
        }
        quote = q;
        rules.push(body);
    }
    if rules.len() < 2 {
        return None;
    }
    Some(Fix {
        title: "Convert rules to a string".into(),
        edits: vec![edit(
            open,
            close + 1,
            format!("{quote}{}{quote}", rules.join("|")),
        )],
        kind: FixKind::RulesToString,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(src: &str, fix: &Fix) -> String {
        let mut out = src.to_string();
        for e in fix.edits.iter().rev() {
            out.replace_range(e.start..e.end, &e.text);
        }
        out
    }

    #[test]
    fn toggle_echo_should_switch_both_ways() {
        let src = "<p>{{ $name }}</p>";
        let fix = &laravel_actions(src, 7, true)[0];
        let raw = apply(src, fix);
        assert_eq!(raw, "<p>{!! $name !!}</p>");
        assert_eq!(apply(&raw, &laravel_actions(&raw, 7, true)[0]), src);
    }

    #[test]
    fn rules_should_convert_between_string_and_array() {
        let src = "<?php $request->validate(['email' => 'required|email']);";
        let at = src.find("required").unwrap();
        let array = apply(src, &laravel_actions(src, at, false)[0]);
        assert_eq!(
            array,
            "<?php $request->validate(['email' => ['required', 'email']]);"
        );
        let at = array.find("'required'").unwrap() + 2;
        let back = laravel_actions(&array, at, false);
        let string = back
            .iter()
            .find(|f| f.kind == FixKind::RulesToString)
            .unwrap();
        assert_eq!(apply(&array, string), src);
    }
}
