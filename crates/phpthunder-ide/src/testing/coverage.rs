//! Code coverage from Clover XML reports (`--coverage-clover`, Codeception
//! `--coverage-xml`): covered and uncovered statement lines per file.

use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;
use std::path::PathBuf;

/// Line coverage of one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileCoverage {
    /// 1-based lines whose statements ran.
    pub covered: Vec<u32>,
    /// 1-based statement lines that never ran.
    pub uncovered: Vec<u32>,
}

impl FileCoverage {
    /// Covered share of the statement lines, 0–100; `None` without any.
    pub fn percent(&self) -> Option<f32> {
        let total = self.covered.len() + self.uncovered.len();
        #[expect(
            clippy::cast_precision_loss,
            reason = "line counts are far below f32 precision limits"
        )]
        (total > 0).then(|| self.covered.len() as f32 * 100.0 / total as f32)
    }
}

/// Coverage of a run: per file, as the interpreter named them.
pub type Coverage = HashMap<PathBuf, FileCoverage>;

/// Value of attribute `name` on an element.
fn attr(element: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned())
        })
}

/// Parses a Clover report: statement lines of every `<file>`.
///
/// # Errors
/// Returns the parser error when the XML is malformed.
pub fn parse_clover(xml: &str) -> Result<Coverage, String> {
    let mut reader = Reader::from_str(xml);
    let mut out = Coverage::new();
    let mut file: Option<PathBuf> = None;
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) if e.name().as_ref() == "file" => {
                file = attr(&e, "name").map(PathBuf::from);
            }
            Event::End(e) if e.name().as_ref() == "file" => file = None,
            Event::Empty(e) | Event::Start(e) if e.name().as_ref() == "line" => {
                let Some(path) = &file else { continue };
                if attr(&e, "type").as_deref() != Some("stmt") {
                    continue;
                }
                let (Some(num), Some(count)) = (
                    attr(&e, "num").and_then(|n| n.parse::<u32>().ok()),
                    attr(&e, "count").and_then(|c| c.parse::<u64>().ok()),
                ) else {
                    continue;
                };
                let entry = out.entry(path.clone()).or_default();
                if count > 0 {
                    entry.covered.push(num);
                } else {
                    entry.uncovered.push(num);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clover_should_split_covered_and_uncovered_statements() {
        let xml = r#"<?xml version="1.0"?>
<coverage><project><file name="/app/src/User.php">
<line num="5" type="method" name="f" count="1"/>
<line num="6" type="stmt" count="3"/>
<line num="7" type="stmt" count="0"/>
</file></project></coverage>"#;
        let c = parse_clover(xml).unwrap();
        let f = &c[&PathBuf::from("/app/src/User.php")];
        assert_eq!((f.covered.clone(), f.uncovered.clone()), (vec![6], vec![7]));
        assert_eq!(f.percent(), Some(50.0));
    }
}
