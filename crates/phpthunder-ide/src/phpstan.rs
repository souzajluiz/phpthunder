//! PHPStan integration: analyse a saved file with the project's PHPStan
//! configuration and turn its findings into problems.

use crate::project::vendor_bin;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One PHPStan finding in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// 1-based line.
    pub line: u32,
    /// PHPStan's message.
    pub message: String,
}

/// `--error-format=json` output: findings grouped by file.
#[derive(Deserialize)]
struct Report {
    /// Absolute path -> findings.
    #[serde(default)]
    files: HashMap<String, FileReport>,
}

/// Findings for one file in the JSON report.
#[derive(Deserialize)]
struct FileReport {
    /// The findings.
    messages: Vec<Message>,
}

/// One finding in the JSON report.
#[derive(Deserialize)]
struct Message {
    /// Text of the finding.
    message: String,
    /// 1-based line; absent for file-level findings.
    line: Option<u32>,
}

/// Whether the project has PHPStan installed.
pub fn phpstan_available(root: &Path) -> bool {
    vendor_bin(root, "phpstan").is_some()
}

/// Parses PHPStan's JSON report into findings for `file`.
///
/// # Errors
/// Returns the JSON error when the output is not a PHPStan report.
pub fn parse_report(json: &str, file: &Path) -> Result<Vec<Finding>, String> {
    // PHPStan may print warnings before the JSON object.
    let start = json.find('{').ok_or("PHPStan returned no report")?;
    let report: Report = serde_json::from_str(&json[start..]).map_err(|e| e.to_string())?;
    let wanted = file.to_string_lossy();
    Ok(report
        .files
        .into_iter()
        .filter(|(path, _)| *path == wanted || Path::new(path).ends_with(file))
        .flat_map(|(_, f)| f.messages)
        .map(|m| Finding {
            line: m.line.unwrap_or(1),
            message: m.message,
        })
        .collect())
}

/// Analyses `file` with the project's PHPStan (and its `phpstan.neon`).
/// Runs the process asynchronously; called after a save.
///
/// # Errors
/// Returns a message when PHPStan is missing, cannot start, or its output
/// cannot be read.
pub async fn analyse(root: PathBuf, file: PathBuf) -> Result<Vec<Finding>, String> {
    let bin = vendor_bin(&root, "phpstan").ok_or("PHPStan is not installed")?;
    let output = tokio::process::Command::new("php")
        .current_dir(&root)
        .arg(&bin)
        .args([
            "analyse",
            "--error-format=json",
            "--no-progress",
            "--no-interaction",
            "--memory-limit=1G",
        ])
        .arg(&file)
        .output()
        .await
        .map_err(|e| format!("cannot run php: {e}"))?;
    parse_report(&String::from_utf8_lossy(&output.stdout), &file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_report_should_return_findings_for_the_file() {
        let json = r#"Note: Using configuration file phpstan.neon.
{"totals":{"errors":0,"file_errors":2},"files":{"/app/src/User.php":{"errors":2,"messages":[{"message":"Undefined variable: $x","line":12,"ignorable":true},{"message":"Method has no return type.","line":3,"ignorable":true}]}},"errors":[]}"#;
        let findings = parse_report(json, Path::new("/app/src/User.php")).unwrap();
        assert_eq!(findings.len(), 2);
        assert_eq!(
            findings[0],
            Finding {
                line: 12,
                message: "Undefined variable: $x".into()
            }
        );
    }
}
