//! Structured data files (JSON, YAML, XML): the JSONPath or XPath of the
//! caret, OpenAPI / JSON Schema `$ref` targets, and YAML mistakes a
//! parser would reject (tab indentation, duplicate keys).

use crate::tr;
use phpthunder::inspection::{Code, Diagnostic, Severity};
use phpthunder::syntax::LineIndex;

/// One step of a path into a document.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Segment {
    /// A mapping key.
    Key(String),
    /// A list position, from 0.
    Index(usize),
}

/// `$.a.b[0]['odd key']` for `path`.
pub fn json_path(path: &[Segment]) -> String {
    let mut out = String::from("$");
    for segment in path {
        match segment {
            Segment::Key(k)
                if !k.is_empty() && k.chars().all(|c| c.is_alphanumeric() || c == '_') =>
            {
                out.push('.');
                out.push_str(k);
            }
            Segment::Key(k) => out.push_str(&format!("['{}']", k.replace('\'', "\\'"))),
            Segment::Index(i) => out.push_str(&format!("[{i}]")),
        }
    }
    out
}

// ----- JSON -------------------------------------------------------------------

/// One open container while scanning JSON.
struct Frame {
    /// An array (else an object).
    array: bool,
    /// The current key of an object.
    key: Option<String>,
    /// The current position in an array.
    index: usize,
}

/// The path that `frames` describe.
fn frames_path(frames: &[Frame]) -> Vec<Segment> {
    frames
        .iter()
        .filter_map(|f| {
            if f.array {
                Some(Segment::Index(f.index))
            } else {
                f.key.clone().map(Segment::Key)
            }
        })
        .collect()
}

/// End offset (after the closing quote) of the JSON string at `at`, and
/// its unescaped-enough content.
fn json_string(src: &str, at: usize) -> (usize, String) {
    let b = src.as_bytes();
    let mut i = at + 1;
    while i < b.len() && b[i] != b'"' {
        i += if b[i] == b'\\' { 2 } else { 1 };
    }
    let end = (i + 1).min(b.len());
    let content = src.get(at + 1..i.min(b.len())).unwrap_or_default();
    (end, content.replace("\\\"", "\"").replace("\\\\", "\\"))
}

/// Walks JSON `src`, calling `on_key(path, key_start)` for every object key
/// with the path that key opens, and stopping when `stop` is reached with
/// the path at that point.
fn walk_json(src: &str, stop: usize, mut on_key: impl FnMut(&[Segment], usize)) -> Vec<Segment> {
    let b = src.as_bytes();
    let mut frames: Vec<Frame> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                let (end, content) = json_string(src, i);
                let colon = src[end..].trim_start().starts_with(':');
                if colon && let Some(top) = frames.last_mut().filter(|f| !f.array) {
                    top.key = Some(content);
                    on_key(&frames_path(&frames), i);
                }
                if (i..end).contains(&stop) {
                    return frames_path(&frames);
                }
                i = end;
                continue;
            }
            b'{' | b'[' => frames.push(Frame {
                array: b[i] == b'[',
                key: None,
                index: 0,
            }),
            b'}' | b']' => {
                frames.pop();
            }
            b',' => {
                if let Some(top) = frames.last_mut() {
                    if top.array {
                        top.index += 1;
                    } else {
                        top.key = None;
                    }
                }
            }
            _ => {}
        }
        if i >= stop {
            break;
        }
        i += 1;
    }
    frames_path(&frames)
}

/// The path of the value or key at `offset` in JSON `src`.
pub fn json_path_at(src: &str, offset: usize) -> Vec<Segment> {
    walk_json(src, offset, |_, _| {})
}

// ----- YAML -------------------------------------------------------------------

/// Leading spaces of a line, and whether a tab appears among them.
fn indent(line: &str) -> (usize, bool) {
    let lead = line.len() - line.trim_start_matches([' ', '\t']).len();
    (lead, line[..lead].contains('\t'))
}

/// The key of a `key: value` YAML line body (quotes removed); `None` for
/// list scalars and other lines.
fn yaml_key(body: &str) -> Option<String> {
    let body = body.trim_end();
    let (key, rest) = if let Some(q) = body.strip_prefix(['"', '\'']) {
        let quote = body.as_bytes()[0] as char;
        let end = q.find(quote)?;
        (q[..end].to_string(), &q[end + 1..])
    } else {
        let colon = body
            .find(": ")
            .or_else(|| body.strip_suffix(':').map(|b| b.len()))?;
        (body[..colon].trim().to_string(), &body[colon..])
    };
    (rest.starts_with(':') && !key.is_empty() && !key.starts_with('#')).then_some(key)
}

/// Walks YAML `src` line by line, calling `visit(path, key_start, line)`
/// for each line that holds a key or list item, with that line's full path.
fn walk_yaml(src: &str, mut visit: impl FnMut(&[Segment], usize, usize)) {
    // (indent, segment): Index entries sit at their dash's column.
    let mut stack: Vec<(usize, Segment)> = Vec::new();
    let mut offset = 0;
    for (n, line) in src.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += line.len();
        let text = line.trim_end_matches(['\n', '\r']);
        let trimmed = text.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("---") {
            continue;
        }
        let (mut col, _) = indent(text);
        let mut body = trimmed;
        loop {
            if let Some(rest) = body
                .strip_prefix("- ")
                .or_else(|| (body == "-").then_some(""))
            {
                // A list item: same-column dashes are siblings; a key at the
                // dash's column (`tags:` then `- a`) owns the list.
                stack.retain(|(i, _)| *i <= col);
                let index = match stack.last() {
                    Some((i, Segment::Index(k))) if *i == col => Some(*k + 1),
                    _ => None,
                };
                if index.is_some() {
                    stack.pop();
                }
                stack.push((col, Segment::Index(index.unwrap_or(0))));
                let skip = body.len() - rest.len();
                col += skip + (rest.len() - rest.trim_start().len());
                body = rest.trim_start();
                if body.is_empty() {
                    visit(&path(&stack), start + text.len() - trimmed.len(), n);
                    break;
                }
                continue;
            }
            if let Some(key) = yaml_key(body) {
                stack.retain(|(i, _)| *i < col);
                stack.push((col, Segment::Key(key)));
            }
            visit(&path(&stack), start + (text.len() - body.len()), n);
            break;
        }
    }
    /// The segments of `stack`.
    fn path(stack: &[(usize, Segment)]) -> Vec<Segment> {
        stack.iter().map(|(_, s)| s.clone()).collect()
    }
}

/// The path of the key or item on the line holding `offset` in YAML `src`.
pub fn yaml_path_at(src: &str, offset: usize) -> Vec<Segment> {
    let target = src[..offset.min(src.len())].matches('\n').count();
    let mut found = Vec::new();
    walk_yaml(src, |path, _, line| {
        if line <= target {
            found = path.to_vec();
        }
    });
    found
}

/// YAML mistakes: tabs in indentation (an error for every parser) and a
/// key repeated in one mapping (the later one silently wins).
pub fn yaml_diagnostics(src: &str) -> Vec<Diagnostic> {
    let lines = LineIndex::new(src);
    let mut out = Vec::new();
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        let (lead, tab) = indent(line);
        if tab && !line.trim().is_empty() {
            out.push(diagnostic(
                &lines,
                src,
                offset,
                offset + lead,
                Severity::Error,
                tr!("yaml-tab-indent"),
            ));
        }
        offset += line.len();
    }
    let mut seen: std::collections::HashSet<Vec<Segment>> = std::collections::HashSet::new();
    walk_yaml(src, |path, start, _| {
        if matches!(path.last(), Some(Segment::Key(_))) && !seen.insert(path.to_vec()) {
            let Some(Segment::Key(key)) = path.last() else {
                return;
            };
            let end = (start + key.len()).min(src.len());
            out.push(diagnostic(
                &lines,
                src,
                start,
                end,
                Severity::Warning,
                tr!("yaml-duplicate-key", key = key.as_str()),
            ));
        }
    });
    out.sort_by_key(|d| d.start);
    out
}

/// A YAML problem over `start..end`.
fn diagnostic(
    lines: &LineIndex,
    src: &str,
    start: usize,
    end: usize,
    severity: Severity,
    message: String,
) -> Diagnostic {
    Diagnostic {
        span: lines.span(src, start, end),
        start,
        end,
        severity,
        message,
        code: Code::External("YAML".into()),
    }
}

// ----- $ref ---------------------------------------------------------------------

/// The `$ref` value on the caret's line, split into the file part (empty
/// for the same document) and the JSON pointer segments.
pub fn ref_at(src: &str, offset: usize) -> Option<(String, Vec<String>)> {
    let offset = offset.min(src.len());
    let start = src[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = src[offset..].find('\n').map_or(src.len(), |i| offset + i);
    let line = &src[start..end];
    let at = line.find("$ref")?;
    let value = line[at + 4..]
        .trim_start_matches(['"', '\''])
        .trim_start()
        .strip_prefix(':')?
        .trim()
        .trim_end_matches(',')
        .trim_matches(['"', '\'']);
    let (file, pointer) = value.split_once('#').unwrap_or((value, ""));
    let segments = pointer
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect();
    Some((file.to_string(), segments))
}

/// Whether `path` names `pointer` (list positions as numbers).
fn same_path(path: &[Segment], pointer: &[String]) -> bool {
    path.len() == pointer.len()
        && path.iter().zip(pointer).all(|(s, p)| match s {
            Segment::Key(k) => k == p,
            Segment::Index(i) => p.parse() == Ok(*i),
        })
}

/// Byte offset of the key that `pointer` names in `src` (YAML when
/// `yaml`, else JSON); the start for an empty pointer.
pub fn resolve_pointer(src: &str, pointer: &[String], yaml: bool) -> Option<usize> {
    if pointer.is_empty() {
        return Some(0);
    }
    let mut found = None;
    if yaml {
        walk_yaml(src, |path, start, _| {
            if found.is_none() && same_path(path, pointer) {
                found = Some(start);
            }
        });
    } else {
        walk_json(src, usize::MAX, |path, start| {
            if found.is_none() && same_path(path, pointer) {
                found = Some(start);
            }
        });
    }
    found
}

// ----- XML ----------------------------------------------------------------------

/// The XPath of the element at `offset` in XML `src`: `/a/b[2]/c`, with a
/// position only where an earlier sibling has the same name.
pub fn xpath_at(src: &str, offset: usize) -> String {
    let b = src.as_bytes();
    // Open elements with their position, and per level the names seen.
    let mut open: Vec<(String, usize)> = Vec::new();
    let mut counts: Vec<std::collections::HashMap<String, usize>> = vec![Default::default()];
    let mut i = 0;
    while i < b.len() && i <= offset {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &src[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").map_or(rest.len(), |e| e + 3);
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            i += rest.find('>').map_or(rest.len(), |e| e + 1);
            continue;
        }
        let close = rest.find('>').map_or(rest.len(), |e| e + 1);
        let tag = &rest[..close];
        if let Some(name) = tag.strip_prefix("</") {
            let name = name.trim_end_matches('>').trim();
            if open.last().is_some_and(|(n, _)| n == name) && i + close <= offset {
                open.pop();
                counts.pop();
            }
        } else {
            let name: String = tag[1..]
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/')
                .collect();
            let n = counts.last_mut().map_or(1, |c| {
                let entry = c.entry(name.clone()).or_insert(0);
                *entry += 1;
                *entry
            });
            let self_closing = tag.ends_with("/>");
            if !self_closing || offset < i + close {
                open.push((name, n));
                counts.push(Default::default());
            }
            if self_closing && offset < i + close {
                break;
            }
        }
        i += close;
    }
    open.iter()
        .map(|(name, n)| {
            if *n > 1 {
                format!("/{name}[{n}]")
            } else {
                format!("/{name}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str =
        r#"{"paths": {"/users": {"get": {"tags": ["a", "b"]}}}, "info": {"title": "x"}}"#;

    const YAML: &str = "openapi: 3.0.0\npaths:\n  /users:\n    get:\n      tags:\n        - a\n        - b\ncomponents:\n  schemas:\n    User:\n      type: object\n";

    #[test]
    fn json_path_should_follow_keys_and_indices() {
        let at = JSON.find("\"b\"").unwrap() + 1;
        assert_eq!(
            json_path(&json_path_at(JSON, at)),
            "$.paths['/users'].get.tags[1]"
        );
    }

    #[test]
    fn yaml_path_should_follow_indentation_and_items() {
        let at = YAML.find("- b").unwrap();
        assert_eq!(
            json_path(&yaml_path_at(YAML, at)),
            "$.paths['/users'].get.tags[1]"
        );
    }

    #[test]
    fn yaml_items_holding_mappings_should_keep_their_keys() {
        let src = "servers:\n  - url: a\n    name: one\n  - url: b\n";
        let at = src.rfind("url").unwrap();
        assert_eq!(json_path(&yaml_path_at(src, at)), "$.servers[1].url");
    }

    #[test]
    fn refs_should_resolve_in_yaml_and_json() {
        let pointer = vec!["components".to_string(), "schemas".into(), "User".into()];
        let at = resolve_pointer(YAML, &pointer, true).unwrap();
        assert!(YAML[at..].starts_with("User:"));
        let json = r#"{"components": {"schemas": {"User": {}}}}"#;
        let at = resolve_pointer(json, &pointer, false).unwrap();
        assert!(json[at..].starts_with("\"User\""));
    }

    #[test]
    fn ref_at_should_split_file_and_pointer() {
        let src = "      $ref: './models.yaml#/components/schemas/User'\n";
        assert_eq!(
            ref_at(src, 10),
            Some((
                "./models.yaml".to_string(),
                vec!["components".into(), "schemas".into(), "User".into()]
            ))
        );
    }

    #[test]
    fn yaml_should_report_tabs_and_duplicate_keys() {
        let src = "a: 1\nb:\n\tc: 2\na: 3\n";
        let codes: Vec<Severity> = yaml_diagnostics(src).iter().map(|d| d.severity).collect();
        assert_eq!(codes, [Severity::Error, Severity::Warning]);
    }

    #[test]
    fn xpath_should_number_repeated_siblings() {
        let src = "<?xml version=\"1.0\"?>\n<root><item/><item><name>x</name></item></root>";
        let at = src.find(">x<").unwrap() + 1;
        assert_eq!(xpath_at(src, at), "/root/item[2]/name");
    }
}
