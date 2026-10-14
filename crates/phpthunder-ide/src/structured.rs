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

// ----- queries ------------------------------------------------------------------

/// Offsets that the JSONPath or XPath `expr` selects in `src`, read as
/// `file_type`; empty for other types and malformed queries (the palette
/// then lists nothing).
pub fn query(src: &str, file_type: crate::filetype::FileType, expr: &str) -> Vec<usize> {
    use crate::filetype::FileType;
    if expr.trim().is_empty() {
        return Vec::new();
    }
    match file_type {
        FileType::Json => eval_json_path(expr, &json_nodes(src)),
        FileType::Yaml => eval_json_path(expr, &yaml_nodes(src)),
        FileType::Xml | FileType::Html => eval_xpath(expr, &xml_elements(src)),
        _ => Ok(Vec::new()),
    }
    .unwrap_or_default()
}

/// Every key and list item of JSON `src`, with its path and offset.
pub fn json_nodes(src: &str) -> Vec<(Vec<Segment>, usize)> {
    let mut nodes = Vec::new();
    walk_json(src, usize::MAX, |path, start| {
        nodes.push((path.to_vec(), start))
    });
    // List items without a key: the first value after `[` or `,`.
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                i = json_string(src, i).0;
                continue;
            }
            b'[' | b',' => {
                let rest = &src[i + 1..];
                let at = i + 1 + (rest.len() - rest.trim_start().len());
                let path = json_path_at(src, at);
                let is_item = matches!(path.last(), Some(Segment::Index(_)))
                    && !matches!(b.get(at), Some(b']' | b'}') | None);
                if is_item && !nodes.iter().any(|(p, _)| *p == path) {
                    nodes.push((path, at));
                }
            }
            _ => {}
        }
        i += 1;
    }
    nodes.sort_by_key(|(_, at)| *at);
    nodes
}

/// Every key and list item of YAML `src`, with its path and offset.
pub fn yaml_nodes(src: &str) -> Vec<(Vec<Segment>, usize)> {
    let mut nodes: Vec<(Vec<Segment>, usize)> = Vec::new();
    walk_yaml(src, |path, start, _| {
        if !path.is_empty() && !nodes.iter().any(|(p, _)| p == path) {
            nodes.push((path.to_vec(), start));
        }
    });
    nodes
}

/// One step of a JSONPath: `.key`, `[n]` or `*`, optionally `..` (any
/// depth).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    /// Preceded by `..`.
    descend: bool,
    /// The key, `None` for `*`.
    key: Option<String>,
    /// A list position.
    index: Option<usize>,
}

/// Parses a JSONPath (`$.a..b[0]['c d'].*`); `None` when it is malformed.
fn parse_json_path(expr: &str) -> Option<Vec<Step>> {
    let mut rest = expr.trim().strip_prefix('$').unwrap_or(expr.trim());
    let mut steps = Vec::new();
    while !rest.is_empty() {
        let descend = rest.starts_with("..");
        if descend {
            rest = &rest[2..];
        } else if let Some(r) = rest.strip_prefix('.') {
            rest = r;
        }
        if let Some(r) = rest.strip_prefix('[') {
            let close = r.find(']')?;
            let inner = r[..close].trim();
            rest = &r[close + 1..];
            let step = if inner == "*" {
                Step {
                    descend,
                    key: None,
                    index: None,
                }
            } else if let Ok(n) = inner.parse() {
                Step {
                    descend,
                    key: None,
                    index: Some(n),
                }
            } else {
                let key = inner.trim_matches(['\'', '"']).to_string();
                Step {
                    descend,
                    key: Some(key),
                    index: None,
                }
            };
            steps.push(step);
            continue;
        }
        let end = rest.find(['.', '[']).unwrap_or(rest.len());
        let name = &rest[..end];
        if name.is_empty() {
            return None;
        }
        rest = &rest[end..];
        steps.push(Step {
            descend,
            key: (name != "*").then(|| name.to_string()),
            index: None,
        });
    }
    Some(steps)
}

/// Whether `segment` is what `step` asks for.
fn step_matches(step: &Step, segment: &Segment) -> bool {
    match (segment, &step.key, step.index) {
        (Segment::Index(i), None, Some(n)) => *i == n,
        (_, None, None) => true,
        (Segment::Key(k), Some(want), None) => k == want,
        _ => false,
    }
}

/// Whether the path `segments` matches `steps` exactly.
fn path_matches(steps: &[Step], segments: &[Segment]) -> bool {
    let Some((step, rest)) = steps.split_first() else {
        return segments.is_empty();
    };
    if step.descend {
        (0..segments.len())
            .any(|i| step_matches(step, &segments[i]) && path_matches(rest, &segments[i + 1..]))
    } else {
        segments
            .split_first()
            .is_some_and(|(first, tail)| step_matches(step, first) && path_matches(rest, tail))
    }
}

/// Offsets of the `nodes` the JSONPath `expr` selects.
///
/// # Errors
/// Returns a message when `expr` is not a JSONPath this evaluator reads.
pub fn eval_json_path(expr: &str, nodes: &[(Vec<Segment>, usize)]) -> Result<Vec<usize>, String> {
    let steps = parse_json_path(expr).ok_or_else(|| tr!("query-invalid"))?;
    Ok(nodes
        .iter()
        .filter(|(path, _)| path_matches(&steps, path))
        .map(|(_, at)| *at)
        .collect())
}

/// One element of an XML document.
#[derive(Debug, Clone)]
pub struct Element {
    /// Tag name.
    name: String,
    /// Position among same-named siblings, from 1.
    index: usize,
    /// Attributes, as written.
    attributes: Vec<(String, String)>,
    /// The enclosing element.
    parent: Option<usize>,
    /// Offset of `<`.
    pub offset: usize,
}

/// Attributes of an opening tag's text (`a="1" b='2'`).
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq]
            .trim_end()
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or_default()
            .to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let Some(end) = after[1..].find(quote) else {
            break;
        };
        out.push((name, after[1..1 + end].to_string()));
        rest = &after[end + 2..];
    }
    out
}

/// Every element of XML `src`, in document order.
pub fn xml_elements(src: &str) -> Vec<Element> {
    let mut out: Vec<Element> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut counts: Vec<std::collections::HashMap<String, usize>> = vec![Default::default()];
    let mut i = 0;
    while let Some(lt) = src[i..].find('<') {
        let at = i + lt;
        let rest = &src[at..];
        let skip = if rest.starts_with("<!--") {
            rest.find("-->").map_or(rest.len(), |e| e + 3)
        } else if rest.starts_with("<![CDATA[") {
            rest.find("]]>").map_or(rest.len(), |e| e + 3)
        } else {
            rest.find('>').map_or(rest.len(), |e| e + 1)
        };
        let tag = &rest[..skip];
        if let Some(name) = tag.strip_prefix("</") {
            let name = name.trim_end_matches('>').trim();
            if let Some(pos) = open.iter().rposition(|&e| out[e].name == name) {
                open.truncate(pos);
                counts.truncate(pos + 1);
            }
        } else if !tag.starts_with("<?") && !tag.starts_with("<!") {
            let name: String = tag[1..]
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/')
                .collect();
            let index = counts.last_mut().map_or(1, |c| {
                let n = c.entry(name.clone()).or_insert(0);
                *n += 1;
                *n
            });
            out.push(Element {
                attributes: attributes(&tag[1 + name.len()..]),
                name,
                index,
                parent: open.last().copied(),
                offset: at,
            });
            if !tag.ends_with("/>") {
                open.push(out.len() - 1);
                counts.push(Default::default());
            }
        }
        i = at + skip.max(1);
    }
    out
}

/// One step of an XPath: a name (or `*`), with an optional position or
/// `[@attribute='value']` test, optionally after `//`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct XStep {
    /// Preceded by `//`.
    descend: bool,
    /// The tag name, `None` for `*`.
    name: Option<String>,
    /// `[n]`.
    index: Option<usize>,
    /// `[@a]` or `[@a='v']`.
    attribute: Option<(String, Option<String>)>,
}

/// Parses an XPath (`/a//b[2]/*[@id='x']`); `None` when it is malformed.
fn parse_xpath(expr: &str) -> Option<Vec<XStep>> {
    let expr = expr.trim();
    let mut steps = Vec::new();
    let mut rest = expr;
    let relative = !rest.starts_with('/');
    let mut first = true;
    while !rest.is_empty() {
        let descend = if rest.starts_with("//") {
            rest = &rest[2..];
            true
        } else if let Some(r) = rest.strip_prefix('/') {
            rest = r;
            false
        } else {
            // A relative path matches at any depth.
            first && relative
        };
        first = false;
        let end = rest.find('/').unwrap_or(rest.len());
        let step = &rest[..end];
        rest = &rest[end..];
        let (name, predicate) = match step.split_once('[') {
            Some((n, p)) => (n, Some(p.strip_suffix(']')?)),
            None => (step, None),
        };
        if name.is_empty() {
            return None;
        }
        let mut x = XStep {
            descend,
            name: (name != "*").then(|| name.to_string()),
            index: None,
            attribute: None,
        };
        match predicate.map(str::trim) {
            Some(p) if p.starts_with('@') => {
                let (a, v) = match p[1..].split_once('=') {
                    Some((a, v)) => (
                        a.trim(),
                        Some(v.trim().trim_matches(['\'', '"']).to_string()),
                    ),
                    None => (p[1..].trim(), None),
                };
                x.attribute = Some((a.to_string(), v));
            }
            Some(p) => x.index = Some(p.parse().ok()?),
            None => {}
        }
        steps.push(x);
    }
    (!steps.is_empty()).then_some(steps)
}

/// Whether `element` is what `step` asks for.
fn xstep_matches(step: &XStep, element: &Element) -> bool {
    step.name.as_ref().is_none_or(|n| *n == element.name)
        && step.index.is_none_or(|i| i == element.index)
        && step.attribute.as_ref().is_none_or(|(a, v)| {
            element
                .attributes
                .iter()
                .any(|(k, val)| k == a && v.as_ref().is_none_or(|v| v == val))
        })
}

/// Whether the chain of elements (root first) matches `steps` exactly.
fn chain_matches(steps: &[XStep], chain: &[&Element]) -> bool {
    let Some((step, rest)) = steps.split_first() else {
        return chain.is_empty();
    };
    if step.descend {
        (0..chain.len())
            .any(|i| xstep_matches(step, chain[i]) && chain_matches(rest, &chain[i + 1..]))
    } else {
        chain
            .split_first()
            .is_some_and(|(first, tail)| xstep_matches(step, first) && chain_matches(rest, tail))
    }
}

/// Offsets of the `elements` the XPath `expr` selects.
///
/// # Errors
/// Returns a message when `expr` is not an XPath this evaluator reads.
pub fn eval_xpath(expr: &str, elements: &[Element]) -> Result<Vec<usize>, String> {
    let steps = parse_xpath(expr).ok_or_else(|| tr!("query-invalid"))?;
    Ok(elements
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let mut chain = Vec::new();
            let mut at = Some(*i);
            while let Some(e) = at {
                chain.push(&elements[e]);
                at = elements[e].parent;
            }
            chain.reverse();
            chain_matches(&steps, &chain)
        })
        .map(|(_, e)| e.offset)
        .collect())
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

    #[test]
    fn json_path_queries_should_select_nodes() {
        let nodes = json_nodes(JSON);
        let found = eval_json_path("$..tags[*]", &nodes).unwrap();
        assert_eq!(found.len(), 2);
        let title = eval_json_path("$.info.title", &nodes).unwrap();
        assert!(JSON[title[0]..].starts_with("\"title\""));
    }

    #[test]
    fn yaml_queries_should_use_the_same_paths() {
        let found = eval_json_path("$.components.schemas.*", &yaml_nodes(YAML)).unwrap();
        assert!(YAML[found[0]..].starts_with("User:"));
    }

    #[test]
    fn xpath_queries_should_follow_names_positions_and_attributes() {
        let src = "<root><item id=\"a\"/><item id=\"b\"><name>x</name></item></root>";
        let elements = xml_elements(src);
        let count = |q: &str| eval_xpath(q, &elements).unwrap().len();
        assert_eq!(
            [
                count("/root/item"),
                count("//name"),
                count("/root/item[2]"),
                count("//item[@id='a']"),
                count("item")
            ],
            [2, 1, 1, 1, 2]
        );
    }

    #[test]
    fn malformed_queries_should_be_errors() {
        assert!(eval_xpath("/a[x", &[]).is_err());
    }
}
