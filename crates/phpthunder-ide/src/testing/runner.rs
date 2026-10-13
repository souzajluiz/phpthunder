//! Running the project's test suite (Pest or PHPUnit) and collecting results.

use super::*;
use crate::project::vendor_bin;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Test framework found in `vendor/bin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framework {
    /// Pest (preferred when both are installed).
    Pest,
    /// PHPUnit.
    PhpUnit,
}

/// What to run.
#[derive(Debug, Clone)]
pub enum Scope {
    /// The whole suite.
    All,
    /// One test file.
    File(PathBuf),
    /// Tests in a file whose name matches a filter (a method or Pest description).
    Filter(PathBuf, String),
}

impl Scope {
    /// Short description for the Tests panel header.
    pub fn label(&self) -> String {
        let name = |p: &Path| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        match self {
            Scope::All => "All tests".into(),
            Scope::File(p) => name(p),
            Scope::Filter(p, f) => format!("{} › {f}", name(p)),
        }
    }
}

/// Result of a test run.
#[derive(Debug, Clone)]
pub struct TestRun {
    /// What was run.
    pub scope: Scope,
    /// Every test case in report order.
    pub cases: Vec<TestCase>,
    /// Wall-clock time of the run.
    pub elapsed: Duration,
    /// Console output, shown when the report is empty (e.g. a fatal error).
    pub output: String,
}

impl TestRun {
    /// Counts as (passed, failed, skipped).
    pub fn counts(&self) -> (usize, usize, usize) {
        self.cases
            .iter()
            .fold((0, 0, 0), |(p, f, s), c| match c.outcome {
                Outcome::Passed => (p + 1, f, s),
                Outcome::Failed(_) => (p, f + 1, s),
                Outcome::Skipped => (p, f, s + 1),
            })
    }
}

/// The test runner installed in the project, if any.
pub fn detect_framework(root: &Path) -> Option<(Framework, PathBuf)> {
    vendor_bin(root, "pest")
        .map(|bin| (Framework::Pest, bin))
        .or_else(|| vendor_bin(root, "phpunit").map(|bin| (Framework::PhpUnit, bin)))
}

/// Runs `scope` with the project's test framework and parses its JUnit
/// report. Runs the process asynchronously; called from a `Task`.
///
/// # Errors
/// Returns a message when no framework is installed or the process cannot
/// start. A run with failing tests is a successful `TestRun`.
pub async fn run_tests(root: PathBuf, scope: Scope) -> Result<TestRun, String> {
    let (_, bin) = detect_framework(&root)
        .ok_or("No test runner found (install pestphp/pest or phpunit/phpunit)")?;
    let report = std::env::temp_dir().join(format!("phpthunder-junit-{}.xml", std::process::id()));
    let _ = std::fs::remove_file(&report);
    let mut cmd = tokio::process::Command::new("php");
    cmd.current_dir(&root)
        .arg(&bin)
        .arg("--colors=never")
        .arg("--log-junit")
        .arg(&report);
    match &scope {
        Scope::All => {}
        Scope::File(path) => {
            cmd.arg(path);
        }
        Scope::Filter(path, filter) => {
            cmd.arg(path).arg("--filter").arg(regex::escape(filter));
        }
    }
    let started = Instant::now();
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("cannot run php: {e}"))?;
    let elapsed = started.elapsed();
    let console = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let cases = match tokio::fs::read_to_string(&report).await {
        Ok(xml) => parse_junit(&xml)?,
        Err(_) => Vec::new(),
    };
    let _ = std::fs::remove_file(&report);
    Ok(TestRun {
        scope,
        cases,
        elapsed,
        output: console,
    })
}

/// The test at `offset` in a PHP test file: the enclosing `test*` method
/// (PHPUnit) or the description of the enclosing `it()` / `test()` call
/// (Pest). `None` outside a test.
pub fn test_at(text: &str, offset: usize) -> Option<String> {
    let tree = phpthunder::syntax::parse(text);
    let mut node = tree.root_node().descendant_for_byte_range(offset, offset)?;
    loop {
        match node.kind() {
            "method_declaration" => {
                let name = node.child_by_field_name("name")?;
                return Some(phpthunder::syntax::text(name, text).to_string());
            }
            "function_call_expression" => {
                let callee = node
                    .child_by_field_name("function")
                    .map(|f| phpthunder::syntax::text(f, text));
                if matches!(callee, Some("it" | "test")) {
                    let args = node.child_by_field_name("arguments")?;
                    let first = args.named_child(0)?.named_child(0)?;
                    let raw = phpthunder::syntax::text(first, text);
                    return Some(raw.trim_matches(|c| c == '\'' || c == '"').to_string());
                }
            }
            _ => {}
        }
        node = node.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_at_should_find_phpunit_methods_and_pest_descriptions() {
        let unit =
            "<?php class UserTest { public function test_name() { $this->assertTrue(true); } }";
        assert_eq!(
            test_at(unit, unit.find("assertTrue").unwrap()).as_deref(),
            Some("test_name")
        );
        let pest = "<?php it('creates users', function () { expect(1)->toBe(1); });";
        assert_eq!(
            test_at(pest, pest.find("expect").unwrap()).as_deref(),
            Some("creates users")
        );
        assert_eq!(test_at(pest, 2), None);
    }
}
