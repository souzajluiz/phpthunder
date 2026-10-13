//! Emmet abbreviations for markup (Blade, HTML, Vue): `ul>li.item*3` and
//! Tab become the HTML they describe. Supported: element names, `.class`,
//! `#id`, `[attr=value]`, `{text}`, children `>`, siblings `+`, climbing
//! up `^`, repetition `*N` with `$` numbering, and grouping `( )`.

/// One element of a parsed abbreviation.
#[derive(Debug, Clone, Default)]
struct Node {
    /// Tag name (`div` when only classes or an id were given).
    tag: String,
    /// `id` attribute.
    id: Option<String>,
    /// `class` values, in order.
    classes: Vec<String>,
    /// Other attributes as (name, value).
    attrs: Vec<(String, Option<String>)>,
    /// Text content.
    text: Option<String>,
    /// Child nodes.
    children: Vec<Node>,
    /// How many copies (`*N`).
    repeat: usize,
}

/// Elements written without a closing tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// HTML element names a bare word expands to (other bare words are left
/// alone, so Tab still indents after ordinary text).
const KNOWN: &[&str] = &[
    "a",
    "abbr",
    "article",
    "aside",
    "audio",
    "b",
    "blockquote",
    "body",
    "br",
    "button",
    "canvas",
    "caption",
    "code",
    "col",
    "dd",
    "details",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "meta",
    "nav",
    "ol",
    "optgroup",
    "option",
    "p",
    "picture",
    "pre",
    "section",
    "select",
    "small",
    "source",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "u",
    "ul",
    "video",
];

/// A tiny recursive-descent parser over the abbreviation's characters.
struct Parser {
    /// The abbreviation.
    chars: Vec<char>,
    /// Read position.
    at: usize,
}

impl Parser {
    /// The next character, without consuming it.
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    /// Characters up to (not including) `stop`, consumed with `stop`.
    fn until(&mut self, stop: char) -> Option<String> {
        let start = self.at;
        while self.peek()? != stop {
            self.at += 1;
        }
        let out: String = self.chars[start..self.at].iter().collect();
        self.at += 1;
        Some(out)
    }

    /// A run of name characters (letters, digits, `-`, `_`, `:`, `$`, `@`).
    fn name(&mut self) -> String {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '$' | '@'))
        {
            self.at += 1;
        }
        self.chars[start..self.at].iter().collect()
    }

    /// Siblings joined by `+`, each with its `>` children; stops at `)`,
    /// `^` or the end.
    fn siblings(&mut self) -> Option<Vec<Node>> {
        let mut out = Vec::new();
        loop {
            let mut group = self.element()?;
            if self.peek() == Some('>') {
                self.at += 1;
                let children = self.siblings()?;
                if let Some(last) = group.last_mut() {
                    last.children = children;
                }
            }
            out.extend(group);
            match self.peek() {
                Some('+') => self.at += 1,
                Some('^') => {
                    self.at += 1;
                    // Climbing up: what follows belongs to the parent level,
                    // which the caller continues as more siblings.
                    out.extend(self.siblings()?);
                    return Some(out);
                }
                _ => return Some(out),
            }
        }
    }

    /// One element (or parenthesized group) with its modifiers.
    fn element(&mut self) -> Option<Vec<Node>> {
        if self.peek() == Some('(') {
            self.at += 1;
            let inner = self.siblings()?;
            if self.peek() != Some(')') {
                return None;
            }
            self.at += 1;
            let times = self.repeat()?;
            return Some((0..times).flat_map(|_| inner.clone()).collect());
        }
        let mut node = Node {
            tag: self.name(),
            repeat: 1,
            ..Node::default()
        };
        loop {
            match self.peek() {
                Some('.') => {
                    self.at += 1;
                    node.classes.push(self.name());
                }
                Some('#') => {
                    self.at += 1;
                    node.id = Some(self.name());
                }
                Some('[') => {
                    self.at += 1;
                    let body = self.until(']')?;
                    for pair in body.split_whitespace() {
                        let (k, v) = match pair.split_once('=') {
                            Some((k, v)) => (k, Some(v.trim_matches(['"', '\'']).to_string())),
                            None => (pair, None),
                        };
                        node.attrs.push((k.to_string(), v));
                    }
                }
                Some('{') => {
                    self.at += 1;
                    node.text = Some(self.until('}')?);
                }
                Some('*') => node.repeat = self.repeat()?,
                _ => break,
            }
        }
        if node.tag.is_empty() {
            if node.classes.is_empty() && node.id.is_none() && node.attrs.is_empty() {
                return None;
            }
            node.tag = "div".to_string();
        }
        Some(vec![node])
    }

    /// `*N` after an element or group; 1 when absent.
    fn repeat(&mut self) -> Option<usize> {
        if self.peek() != Some('*') {
            return Some(1);
        }
        self.at += 1;
        let digits = self.name();
        digits.parse().ok().filter(|n| (1..=200).contains(n))
    }
}

/// `$` in `s` replaced by the 1-based copy number `n`.
fn number(s: &str, n: usize) -> String {
    s.replace('$', &n.to_string())
}

/// Renders `nodes` at `depth`, one element per line, writing `|` where the
/// caret goes (the first empty spot).
fn render(nodes: &[Node], depth: usize, unit: &str, out: &mut String) {
    for node in nodes {
        for n in 1..=node.repeat {
            let indent = unit.repeat(depth);
            out.push_str(&indent);
            out.push('<');
            out.push_str(&number(&node.tag, n));
            if let Some(id) = &node.id {
                out.push_str(&format!(" id=\"{}\"", number(id, n)));
            }
            if !node.classes.is_empty() {
                let classes: Vec<String> = node.classes.iter().map(|c| number(c, n)).collect();
                out.push_str(&format!(" class=\"{}\"", classes.join(" ")));
            }
            for (k, v) in &node.attrs {
                match v {
                    Some(v) => out.push_str(&format!(" {k}=\"{}\"", number(v, n))),
                    None => out.push_str(&format!(" {k}=\"|\"")),
                }
            }
            if node.tag == "a" && !node.attrs.iter().any(|(k, _)| k == "href") {
                out.push_str(" href=\"|\"");
            }
            if node.tag == "img" && !node.attrs.iter().any(|(k, _)| k == "src") {
                out.push_str(" src=\"|\" alt=\"\"");
            }
            if VOID.contains(&node.tag.as_str()) {
                out.push_str(">\n");
                continue;
            }
            out.push('>');
            if node.children.is_empty() {
                let text = node
                    .text
                    .as_deref()
                    .map(|t| number(t, n))
                    .unwrap_or_else(|| "|".into());
                out.push_str(&text);
            } else {
                out.push('\n');
                if let Some(text) = &node.text {
                    out.push_str(&unit.repeat(depth + 1));
                    out.push_str(&number(text, n));
                    out.push('\n');
                }
                render(&node.children, depth + 1, unit, out);
                out.push_str(&indent);
            }
            out.push_str(&format!("</{}>\n", number(&node.tag, n)));
        }
    }
}

/// The abbreviation ending at the end of `before`: back to whitespace or a
/// `>` that closes a tag (outside `[]`/`{}`).
fn abbreviation(before: &str) -> &str {
    let mut depth = 0i32;
    let mut start = before.len();
    for (i, c) in before.char_indices().rev() {
        match c {
            ']' | '}' | ')' => depth += 1,
            '[' | '{' | '(' => depth -= 1,
            c if depth == 0 && c.is_whitespace() => break,
            '<' | '"' | '\'' if depth == 0 => break,
            _ => {}
        }
        start = i;
    }
    &before[start..]
}

/// The expansion of the Emmet abbreviation that ends at `offset`, as
/// (start of the abbreviation, HTML with the caret marked by `|`), with
/// lines after the first indented by `indent`. `None` when the text before
/// the caret is not an abbreviation worth expanding.
pub fn expand(text: &str, offset: usize, indent: &str, unit: &str) -> Option<(usize, String)> {
    let before = text.get(..offset)?;
    let abbr = abbreviation(before);
    let first = abbr.chars().next()?;
    if !(first.is_ascii_alphabetic() || matches!(first, '.' | '#' | '(')) {
        return None;
    }
    let operators = abbr.contains(['.', '#', '>', '+', '*', '[', '{', '^', '(']);
    if !operators && !KNOWN.contains(&abbr) {
        return None;
    }
    let mut parser = Parser {
        chars: abbr.chars().collect(),
        at: 0,
    };
    let nodes = parser.siblings()?;
    if parser.at != parser.chars.len() {
        return None;
    }
    let mut html = String::new();
    render(&nodes, 0, unit, &mut html);
    let html = html
        .trim_end_matches('\n')
        .replace('\n', &format!("\n{indent}"));
    Some((offset - abbr.len(), html))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(abbr: &str) -> String {
        expand(abbr, abbr.len(), "", "    ")
            .map(|e| e.1)
            .unwrap_or_default()
    }

    #[test]
    fn expand_should_build_children_with_numbering() {
        assert_eq!(
            run("ul>li.item$*2"),
            "<ul>\n    <li class=\"item1\">|</li>\n    <li class=\"item2\">|</li>\n</ul>"
        );
    }

    #[test]
    fn expand_should_default_to_div_for_a_class() {
        assert_eq!(run(".card#main"), "<div id=\"main\" class=\"card\">|</div>");
    }

    #[test]
    fn expand_should_handle_siblings_text_and_attributes() {
        assert_eq!(
            run("h1{Title}+a[href=/x]"),
            "<h1>Title</h1>\n<a href=\"/x\">|</a>"
        );
    }

    #[test]
    fn expand_should_leave_plain_words_alone() {
        assert_eq!(expand("hello", 5, "", "    "), None);
    }

    #[test]
    fn expand_should_close_void_elements_once() {
        assert_eq!(run("input[type=text]"), "<input type=\"text\">");
    }
}
