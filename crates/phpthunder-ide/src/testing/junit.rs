//! Reading JUnit XML reports, the format PHPUnit and Pest write with
//! `--log-junit`.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::path::PathBuf;

/// Outcome of one test case.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The test passed.
    Passed,
    /// An assertion failed or an error was thrown; carries the message.
    Failed(String),
    /// The test was skipped or marked incomplete.
    Skipped,
}

/// One test case from the report.
#[derive(Debug, Clone)]
pub struct TestCase {
    /// Test name (method name, or the Pest description).
    pub name: String,
    /// Test class (or Pest file-based class name).
    pub class: String,
    /// File declaring the test, when the report says.
    pub file: Option<PathBuf>,
    /// 1-based line of the test, when the report says.
    pub line: Option<u32>,
    /// Seconds the test took.
    pub time: f64,
    /// Result.
    pub outcome: Outcome,
}

/// Value of attribute `name` on an element, unescaped.
fn attr(element: &BytesStart, name: &str) -> Option<String> {
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

/// Parses a JUnit report into test cases, in report order.
///
/// # Errors
/// Returns the parser error when the XML is malformed.
pub fn parse_junit(xml: &str) -> Result<Vec<TestCase>, String> {
    let mut reader = Reader::from_str(xml);
    let mut cases = Vec::new();
    let mut current: Option<TestCase> = None;
    // Text collected inside <failure>/<error> for the message body.
    let mut in_failure = false;
    let mut failure_text = String::new();
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == "testcase" => {
                let case = TestCase {
                    name: attr(&e, "name").unwrap_or_default(),
                    class: attr(&e, "class")
                        .or_else(|| attr(&e, "classname"))
                        .unwrap_or_default(),
                    file: attr(&e, "file").map(PathBuf::from),
                    line: attr(&e, "line").and_then(|l| l.parse().ok()),
                    time: attr(&e, "time").and_then(|t| t.parse().ok()).unwrap_or(0.0),
                    outcome: Outcome::Passed,
                };
                // `<testcase .../>` has no children: it is complete.
                if let Some(done) = current.replace(case) {
                    cases.push(done);
                }
            }
            Event::Start(e) | Event::Empty(e)
                if matches!(e.name().as_ref(), "failure" | "error") =>
            {
                in_failure = true;
                failure_text.clear();
                if let (Some(case), Some(message)) = (current.as_mut(), attr(&e, "message")) {
                    case.outcome = Outcome::Failed(message);
                } else if let Some(case) = current.as_mut() {
                    case.outcome = Outcome::Failed(String::new());
                }
            }
            Event::Start(e) | Event::Empty(e)
                if matches!(e.name().as_ref(), "skipped" | "incomplete") =>
            {
                if let Some(case) = current.as_mut() {
                    case.outcome = Outcome::Skipped;
                }
            }
            Event::Text(t) if in_failure => {
                let raw = t.xml10_content();
                failure_text
                    .push_str(&quick_xml::escape::unescape(&raw).map_err(|e| e.to_string())?);
            }
            Event::CData(t) if in_failure => {
                failure_text.push_str(&t.xml10_content());
            }
            Event::End(e) if matches!(e.name().as_ref(), "failure" | "error") => {
                in_failure = false;
                if let Some(case) = current.as_mut()
                    && let Outcome::Failed(message) = &mut case.outcome
                    && message.is_empty()
                {
                    *message = failure_text.trim().to_string();
                }
            }
            Event::End(e) if e.name().as_ref() == "testcase" => {
                if let Some(case) = current.take() {
                    cases.push(case);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if let Some(case) = current.take() {
        cases.push(case);
    }
    Ok(cases)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="Tests\Unit\UserTest" file="/app/tests/Unit/UserTest.php" tests="3">
    <testcase name="test_name" class="Tests\Unit\UserTest" file="/app/tests/Unit/UserTest.php" line="12" time="0.010"/>
    <testcase name="test_email" class="Tests\Unit\UserTest" file="/app/tests/Unit/UserTest.php" line="20" time="0.020">
      <failure type="PHPUnit\Framework\ExpectationFailedException">Failed asserting that 'a' is 'b'.
/app/tests/Unit/UserTest.php:22</failure>
    </testcase>
    <testcase name="test_later" class="Tests\Unit\UserTest" line="30" time="0">
      <skipped/>
    </testcase>
  </testsuite>
</testsuites>"#;

    #[test]
    fn parse_junit_should_read_outcomes_and_locations() {
        let cases = parse_junit(REPORT).unwrap();
        assert_eq!(cases.len(), 3);
        assert_eq!(cases[0].outcome, Outcome::Passed);
        assert_eq!(cases[0].line, Some(12));
        assert!(
            matches!(&cases[1].outcome, Outcome::Failed(m) if m.starts_with("Failed asserting"))
        );
        assert_eq!(cases[2].outcome, Outcome::Skipped);
    }
}
