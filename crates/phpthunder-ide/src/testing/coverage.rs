//! Code coverage: from Clover XML reports (`--coverage-clover`, Codeception
//! `--coverage-xml`), or, for runners without coverage support (Behat,
//! PHPSpec), from a script PHP prepends to the run that collects line
//! coverage with PCOV or Xdebug and writes it as JSON when the process
//! ends.

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

/// The `auto_prepend_file` script collecting line coverage of the
/// project's own files (under `root`, outside `vendor/`) with PCOV, else
/// Xdebug, and writing `{file: {line: 0|1}}` JSON to `out` at shutdown.
/// Paths are as the interpreter sees them. Without either extension it
/// does nothing.
pub fn prepend_script(root: &str, out: &str) -> String {
    let quote = |s: &str| format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"));
    format!(
        r#"<?php
(static function () {{
    $out = {out};
    $root = rtrim({root}, '/') . '/';
    if (extension_loaded('pcov')) {{
        \pcov\start();
        $driver = 'pcov';
    }} elseif (function_exists('xdebug_start_code_coverage')) {{
        xdebug_start_code_coverage(XDEBUG_CC_UNUSED | XDEBUG_CC_DEAD_CODE);
        $driver = 'xdebug';
    }} else {{
        return;
    }}
    register_shutdown_function(static function () use ($out, $root, $driver) {{
        if ($driver === 'pcov') {{
            \pcov\stop();
            $data = \pcov\collect(\pcov\inclusive);
        }} else {{
            $data = xdebug_get_code_coverage();
            xdebug_stop_code_coverage(false);
        }}
        $result = [];
        foreach ($data as $file => $lines) {{
            if (strpos($file, $root) !== 0 || strpos($file, '/vendor/') !== false || $file === __FILE__) {{
                continue;
            }}
            foreach ($lines as $line => $hits) {{
                if ($hits !== -2) {{
                    $result[$file][$line] = $hits > 0 ? 1 : 0;
                }}
            }}
        }}
        file_put_contents($out, json_encode($result));
    }});
}})();
"#,
        out = quote(out),
        root = quote(root),
    )
}

/// Reads the JSON the prepend script writes: executable lines per file,
/// `1` when they ran.
///
/// # Errors
/// Returns the JSON error when the text is not such a report.
pub fn parse_line_json(text: &str) -> Result<Coverage, String> {
    let json: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut out = Coverage::new();
    for (file, lines) in json.as_object().into_iter().flatten() {
        let entry = out.entry(PathBuf::from(file)).or_default();
        for (line, hit) in lines.as_object().into_iter().flatten() {
            let Ok(line) = line.parse::<u32>() else {
                continue;
            };
            if hit.as_u64().unwrap_or(0) > 0 {
                entry.covered.push(line);
            } else {
                entry.uncovered.push(line);
            }
        }
        entry.covered.sort_unstable();
        entry.uncovered.sort_unstable();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_json_should_split_covered_and_uncovered_lines() {
        let c = parse_line_json(r#"{"/app/src/Cart.php":{"4":1,"5":0,"9":1}}"#).unwrap();
        let f = &c[&PathBuf::from("/app/src/Cart.php")];
        assert_eq!(
            (f.covered.clone(), f.uncovered.clone()),
            (vec![4, 9], vec![5])
        );
    }

    #[test]
    fn prepend_script_should_quote_paths() {
        let script = prepend_script("/var/www/it's", "/tmp/cov.json");
        assert!(script.contains(r"$root = rtrim('/var/www/it\'s', '/')"));
        assert!(script.contains("$out = '/tmp/cov.json';"));
        assert!(script.contains(r"\pcov\start();"));
    }

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
