//! A small XML tree for DBGp responses.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// An element with its attributes, text and child elements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Element {
    /// Tag name, namespace prefix included (`xdebug:message`).
    pub name: String,
    /// Attributes in document order, values unescaped.
    pub attrs: Vec<(String, String)>,
    /// Text and CDATA content, concatenated.
    pub text: String,
    /// Child elements.
    pub children: Vec<Element>,
}

impl Element {
    /// Value of attribute `name`.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Child elements named `name`.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |c| c.name == name)
    }
}

/// Element for a start tag.
fn element(start: &BytesStart) -> Element {
    Element {
        name: start.name().as_ref().to_string(),
        attrs: start
            .attributes()
            .flatten()
            .map(|a| {
                let key = a.key.as_ref().to_string();
                let value = a
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map(|v| v.into_owned())
                    .unwrap_or_default();
                (key, value)
            })
            .collect(),
        ..Element::default()
    }
}

/// Parses an XML document into its root element.
///
/// # Errors
/// Returns the parser error, or a message when there is no root element.
pub(crate) fn parse_xml(xml: &str) -> Result<Element, String> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Element> = Vec::new();
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) => stack.push(element(&e)),
            Event::Empty(e) => {
                let done = element(&e);
                match stack.last_mut() {
                    Some(parent) => parent.children.push(done),
                    None => return Ok(done),
                }
            }
            Event::End(_) => {
                let done = stack.pop().ok_or("unbalanced XML")?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(done),
                    None => return Ok(done),
                }
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    let raw = t.xml10_content();
                    top.text
                        .push_str(&quick_xml::escape::unescape(&raw).map_err(|e| e.to_string())?);
                }
            }
            Event::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.xml10_content());
                }
            }
            Event::Eof => return Err("no root element".into()),
            _ => {}
        }
    }
}
