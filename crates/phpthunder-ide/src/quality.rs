//! Quality tools run on save: PHPStan, Psalm, PHP_CodeSniffer,
//! PHP-CS-Fixer, PHPMD and Laravel Pint for PHP, ESLint and Stylelint for
//! scripts and styles, each when the project installs it (and, for the
//! configurable ones, configures it). Their findings become problems of
//! the saved file. PHP tools start through the project's interpreter, so
//! tools inside a container work too; Node tools run from `node_modules`.

use crate::interpreter::Interpreter;
use crate::project::vendor_bin;
use crate::tr;
use phpthunder::inspection::Severity;
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A quality tool the IDE knows how to run and read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    /// PHPStan.
    PhpStan,
    /// Psalm.
    Psalm,
    /// PHP_CodeSniffer (`phpcs`).
    Phpcs,
    /// PHP-CS-Fixer (dry run).
    PhpCsFixer,
    /// PHP Mess Detector.
    Phpmd,
    /// Laravel Pint (dry run).
    Pint,
    /// ESLint, for JavaScript, TypeScript and single-file components.
    Eslint,
    /// Stylelint, for CSS, Sass and Less.
    Stylelint,
}

/// One finding of a tool in the analysed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// 1-based line.
    pub line: u32,
    /// The tool's message.
    pub message: String,
    /// How serious the tool says it is.
    pub severity: Severity,
}

/// Every tool, in the order they run.
pub const TOOLS: [Tool; 8] = [
    Tool::PhpStan,
    Tool::Psalm,
    Tool::Phpcs,
    Tool::PhpCsFixer,
    Tool::Phpmd,
    Tool::Pint,
    Tool::Eslint,
    Tool::Stylelint,
];

/// Extensions ESLint checks.
const SCRIPT_EXTENSIONS: &[&str] = &[
    "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "vue", "svelte", "astro",
];

/// Extensions Stylelint checks.
const STYLE_EXTENSIONS: &[&str] = &["css", "scss", "sass", "less", "pcss", "postcss"];

/// ESLint configuration files (flat and legacy).
const ESLINT_CONFIGS: &[&str] = &[
    "eslint.config.js",
    "eslint.config.mjs",
    "eslint.config.cjs",
    "eslint.config.ts",
    ".eslintrc",
    ".eslintrc.js",
    ".eslintrc.cjs",
    ".eslintrc.json",
    ".eslintrc.yml",
    ".eslintrc.yaml",
];

/// Stylelint configuration files.
const STYLELINT_CONFIGS: &[&str] = &[
    ".stylelintrc",
    ".stylelintrc.json",
    ".stylelintrc.yml",
    ".stylelintrc.yaml",
    ".stylelintrc.js",
    ".stylelintrc.cjs",
    ".stylelintrc.mjs",
    "stylelint.config.js",
    "stylelint.config.cjs",
    "stylelint.config.mjs",
];

/// Whether `package.json` in `root` has the top-level `key` (inline tool
/// configuration such as `eslintConfig`).
fn package_json_has(root: &Path, key: &str) -> bool {
    std::fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| v.get(key).is_some())
}

/// Whether any of `names` exists in the project root.
fn has_config(root: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| root.join(n).is_file())
}

impl Tool {
    /// Display name, also the prefix of its problems.
    pub fn name(self) -> &'static str {
        match self {
            Tool::PhpStan => "PHPStan",
            Tool::Psalm => "Psalm",
            Tool::Phpcs => "PHP_CodeSniffer",
            Tool::PhpCsFixer => "PHP-CS-Fixer",
            Tool::Phpmd => "PHPMD",
            Tool::Pint => "Pint",
            Tool::Eslint => "ESLint",
            Tool::Stylelint => "Stylelint",
        }
    }

    /// Whether the tool checks files like `path`: PHP tools take `.php`
    /// (not Blade), ESLint scripts, Stylelint styles.
    pub fn applies_to(self, path: &Path) -> bool {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match self {
            Tool::Eslint => SCRIPT_EXTENSIONS.contains(&ext.as_str()),
            Tool::Stylelint => STYLE_EXTENSIONS.contains(&ext.as_str()),
            _ => ext == "php" && !path.to_string_lossy().ends_with(".blade.php"),
        }
    }

    /// Whether the tool runs from `node_modules` on the host rather than
    /// through the PHP interpreter.
    fn is_node(self) -> bool {
        matches!(self, Tool::Eslint | Tool::Stylelint)
    }

    /// The `vendor/bin` script, also its name in `disabled_tools`.
    pub fn bin(self) -> &'static str {
        match self {
            Tool::PhpStan => "phpstan",
            Tool::Psalm => "psalm",
            Tool::Phpcs => "phpcs",
            Tool::PhpCsFixer => "php-cs-fixer",
            Tool::Phpmd => "phpmd",
            Tool::Pint => "pint",
            Tool::Eslint => "eslint",
            Tool::Stylelint => "stylelint",
        }
    }

    /// The installed script, when the tool is installed and configured:
    /// Psalm, PHPCS, PHP-CS-Fixer and PHPMD need their config file (their
    /// defaults would flood the problems list); PHPStan and Pint do not.
    pub fn available(self, root: &Path) -> Option<PathBuf> {
        let configured = match self {
            Tool::PhpStan | Tool::Pint => true,
            Tool::Psalm => has_config(root, &["psalm.xml", "psalm.xml.dist"]),
            Tool::Phpcs => has_config(
                root,
                &[
                    "phpcs.xml",
                    "phpcs.xml.dist",
                    ".phpcs.xml",
                    ".phpcs.xml.dist",
                ],
            ),
            Tool::PhpCsFixer => has_config(root, &[".php-cs-fixer.php", ".php-cs-fixer.dist.php"]),
            Tool::Phpmd => has_config(root, &["phpmd.xml", "phpmd.xml.dist", ".phpmd.xml"]),
            Tool::Eslint => {
                has_config(root, ESLINT_CONFIGS) || package_json_has(root, "eslintConfig")
            }
            Tool::Stylelint => {
                has_config(root, STYLELINT_CONFIGS) || package_json_has(root, "stylelint")
            }
        };
        if self.is_node() {
            return configured
                .then(|| crate::formatter::node_bin(root, self.bin()))
                .flatten();
        }
        configured.then(|| vendor_bin(root, self.bin())).flatten()
    }

    /// Arguments after the script for analysing `file`.
    fn arguments(self, root: &Path, file: &Path) -> Vec<OsString> {
        let file = file.as_os_str().to_owned();
        let args: Vec<OsString> = match self {
            Tool::PhpStan => vec![
                "analyse".into(),
                "--error-format=json".into(),
                "--no-progress".into(),
                "--no-interaction".into(),
                "--memory-limit=1G".into(),
                file,
            ],
            Tool::Psalm => vec![
                "--output-format=json".into(),
                "--no-progress".into(),
                "--no-cache".into(),
                file,
            ],
            Tool::Phpcs => vec!["--report=json".into(), "-q".into(), file],
            Tool::PhpCsFixer => vec![
                "fix".into(),
                "--dry-run".into(),
                "--diff".into(),
                "--format=json".into(),
                "--using-cache=no".into(),
                file,
            ],
            Tool::Phpmd => {
                let ruleset = ["phpmd.xml", "phpmd.xml.dist", ".phpmd.xml"]
                    .into_iter()
                    .map(|n| root.join(n))
                    .find(|p| p.is_file())
                    .map_or_else(
                        || "cleancode,codesize,design,naming,unusedcode".into(),
                        |p| p.into_os_string(),
                    );
                vec![file, "json".into(), ruleset]
            }
            Tool::Pint => vec!["--test".into(), "--format=json".into(), file],
            Tool::Eslint => vec!["--format".into(), "json".into(), file],
            Tool::Stylelint => vec!["--formatter".into(), "json".into(), file],
        };
        args
    }

    /// Reads the tool's report into findings for `file` (as the
    /// interpreter names it).
    ///
    /// # Errors
    /// Returns the JSON error when the output is not a report.
    pub fn parse(self, output: &str, file: &Path) -> Result<Vec<Finding>, String> {
        // Tools may print notes before the JSON document.
        let start = output
            .find(['{', '['])
            .ok_or_else(|| tr!("error-quality-no-report", tool = self.name()))?;
        let json: Value = serde_json::from_str(&output[start..]).map_err(|e| e.to_string())?;
        let wanted = file.to_string_lossy();
        let same =
            |path: &str| path == wanted || Path::new(path).ends_with(file) || file.ends_with(path);
        let line = |v: &Value| u32::try_from(v.as_u64().unwrap_or(1)).unwrap_or(1).max(1);
        let text = |v: &Value| v.as_str().unwrap_or_default().trim().to_string();
        let mut out = Vec::new();
        match self {
            Tool::PhpStan => {
                for (path, report) in json["files"].as_object().into_iter().flatten() {
                    if !same(path) {
                        continue;
                    }
                    for m in report["messages"].as_array().into_iter().flatten() {
                        out.push(Finding {
                            line: line(&m["line"]),
                            message: text(&m["message"]),
                            severity: Severity::Error,
                        });
                    }
                }
            }
            Tool::Psalm => {
                for m in json.as_array().into_iter().flatten() {
                    if !same(m["file_path"].as_str().unwrap_or_default()) {
                        continue;
                    }
                    let severity = if m["severity"] == "error" {
                        Severity::Error
                    } else {
                        Severity::Warning
                    };
                    out.push(Finding {
                        line: line(&m["line_from"]),
                        message: format!("{} ({})", text(&m["message"]), text(&m["type"])),
                        severity,
                    });
                }
            }
            Tool::Phpcs => {
                for (path, report) in json["files"].as_object().into_iter().flatten() {
                    if !same(path) {
                        continue;
                    }
                    for m in report["messages"].as_array().into_iter().flatten() {
                        let severity = if m["type"] == "ERROR" {
                            Severity::Warning
                        } else {
                            Severity::Weak
                        };
                        out.push(Finding {
                            line: line(&m["line"]),
                            message: format!("{} ({})", text(&m["message"]), text(&m["source"])),
                            severity,
                        });
                    }
                }
            }
            Tool::Phpmd => {
                for f in json["files"].as_array().into_iter().flatten() {
                    if !same(f["file"].as_str().unwrap_or_default()) {
                        continue;
                    }
                    for v in f["violations"].as_array().into_iter().flatten() {
                        out.push(Finding {
                            line: line(&v["beginLine"]),
                            message: format!("{} ({})", text(&v["description"]), text(&v["rule"])),
                            severity: Severity::Weak,
                        });
                    }
                }
            }
            Tool::Eslint => {
                for f in json.as_array().into_iter().flatten() {
                    if !same(f["filePath"].as_str().unwrap_or_default()) {
                        continue;
                    }
                    for m in f["messages"].as_array().into_iter().flatten() {
                        let rule = text(&m["ruleId"]);
                        out.push(Finding {
                            line: line(&m["line"]),
                            message: if rule.is_empty() {
                                text(&m["message"])
                            } else {
                                format!("{} ({rule})", text(&m["message"]))
                            },
                            severity: if m["severity"] == 2 {
                                Severity::Warning
                            } else {
                                Severity::Weak
                            },
                        });
                    }
                }
            }
            Tool::Stylelint => {
                for f in json.as_array().into_iter().flatten() {
                    if !same(f["source"].as_str().unwrap_or_default()) {
                        continue;
                    }
                    for w in f["warnings"].as_array().into_iter().flatten() {
                        out.push(Finding {
                            line: line(&w["line"]),
                            message: text(&w["text"]),
                            severity: if w["severity"] == "error" {
                                Severity::Warning
                            } else {
                                Severity::Weak
                            },
                        });
                    }
                }
            }
            Tool::PhpCsFixer | Tool::Pint => {
                for f in json["files"].as_array().into_iter().flatten() {
                    // PHP-CS-Fixer writes `name`/`appliedFixers`, Pint
                    // `path` (project-relative)/`fixers`.
                    let name = f["name"].as_str().or_else(|| f["path"].as_str());
                    if !same(name.unwrap_or_default()) {
                        continue;
                    }
                    let fixers: Vec<String> = f["appliedFixers"]
                        .as_array()
                        .or_else(|| f["fixers"].as_array())
                        .into_iter()
                        .flatten()
                        .map(text)
                        .collect();
                    let message = if fixers.is_empty() {
                        tr!("quality-not-formatted")
                    } else {
                        tr!("quality-not-formatted-rules", rules = fixers.join(", "))
                    };
                    let lines = diff_lines(f["diff"].as_str().unwrap_or_default());
                    for l in if lines.is_empty() { vec![1] } else { lines } {
                        out.push(Finding {
                            line: l,
                            message: message.clone(),
                            severity: Severity::Weak,
                        });
                    }
                }
            }
        }
        Ok(out)
    }
}

/// First changed line of each hunk of a unified diff (`@@ -12,7 +12,7 @@`),
/// in the original file.
fn diff_lines(diff: &str) -> Vec<u32> {
    diff.lines()
        .filter_map(|l| l.strip_prefix("@@ -"))
        .filter_map(|rest| {
            let start = rest.split([',', ' ']).next()?.parse::<u32>().ok()?;
            Some(start.max(1))
        })
        .collect()
}

/// The tools to run for the project: available and not disabled in the
/// project settings.
pub fn enabled_tools(root: &Path) -> Vec<Tool> {
    let disabled = crate::interpreter::project_config(root).disabled_tools;
    TOOLS
        .into_iter()
        .filter(|t| !disabled.iter().any(|d| d.eq_ignore_ascii_case(t.bin())))
        .filter(|t| t.available(root).is_some())
        .collect()
}

/// Analyses `file` with `tool` (and the project's configuration for it),
/// asynchronously; called after a save.
///
/// # Errors
/// Returns a message when the tool is missing, cannot start, or its output
/// cannot be read.
pub async fn analyse(tool: Tool, root: PathBuf, file: PathBuf) -> Result<Vec<Finding>, String> {
    let bin = tool
        .available(&root)
        .ok_or_else(|| tr!("error-quality-missing", tool = tool.name()))?;
    if tool.is_node() {
        let output = tokio::process::Command::new(&bin)
            .args(tool.arguments(&root, &file))
            .current_dir(&root)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| format!("{}: {e}", tool.name()))?;
        // Stylelint 16 writes its JSON report to stderr.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let report = if stdout.trim().is_empty() {
            stderr
        } else {
            stdout
        };
        return match tool.parse(&report, &file) {
            Err(_) if report.trim().is_empty() => Ok(Vec::new()),
            other => other,
        };
    }
    let interpreter = Interpreter::of(&root);
    let mut args = vec![bin.into_os_string()];
    args.extend(tool.arguments(&root, &file));
    let output = interpreter
        .command(&root, "php", &args)
        .output()
        .await
        .map_err(|e| tr!("error-run-php", error = e.to_string()))?;
    // Reports name files as the interpreter sees them.
    let remote = PathBuf::from(interpreter.to_remote(&root, &file));
    let stdout = String::from_utf8_lossy(&output.stdout);
    match tool.parse(&stdout, &remote) {
        // A clean run of some tools prints nothing.
        Err(_) if stdout.trim().is_empty() && output.status.success() => Ok(Vec::new()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eslint_report_should_yield_rule_findings() {
        let json = r#"[{"filePath":"/p/app.js","messages":[{"ruleId":"no-unused-vars","severity":2,"message":"'x' is unused.","line":3}]}]"#;
        let findings = Tool::Eslint.parse(json, Path::new("/p/app.js")).unwrap();
        assert_eq!(
            findings,
            [Finding {
                line: 3,
                message: "'x' is unused. (no-unused-vars)".into(),
                severity: Severity::Warning,
            }]
        );
    }

    #[test]
    fn stylelint_report_should_yield_findings() {
        let json = r#"[{"source":"/p/a.css","warnings":[{"line":2,"text":"Unexpected unit (length-zero-no-unit)","severity":"warning"}]}]"#;
        let findings = Tool::Stylelint.parse(json, Path::new("/p/a.css")).unwrap();
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn tools_should_apply_to_their_file_types() {
        let applies = |t: Tool, f: &str| t.applies_to(Path::new(f));
        assert_eq!(
            [
                applies(Tool::Eslint, "a.vue"),
                applies(Tool::Stylelint, "a.scss"),
                applies(Tool::PhpStan, "a.php"),
                applies(Tool::PhpStan, "a.blade.php"),
                applies(Tool::Eslint, "a.php"),
            ],
            [true, true, true, false, false]
        );
    }

    #[test]
    fn phpstan_report_should_yield_findings_for_the_file() {
        let json = r#"Note: Using configuration file phpstan.neon.
{"totals":{"errors":0,"file_errors":1},"files":{"/app/src/User.php":{"errors":1,"messages":[{"message":"Undefined variable: $x","line":12}]}},"errors":[]}"#;
        let findings = Tool::PhpStan
            .parse(json, Path::new("/app/src/User.php"))
            .unwrap();
        assert_eq!(
            findings,
            [Finding {
                line: 12,
                message: "Undefined variable: $x".into(),
                severity: Severity::Error
            }]
        );
    }

    #[test]
    fn psalm_report_should_keep_type_and_severity() {
        let json = r#"[{"severity":"info","line_from":3,"type":"MissingReturnType","message":"Method has no return type","file_path":"/app/a.php"}]"#;
        let f = Tool::Psalm.parse(json, Path::new("/app/a.php")).unwrap();
        assert_eq!(f[0].line, 3);
        assert_eq!(f[0].severity, Severity::Warning);
        assert!(f[0].message.contains("MissingReturnType"));
    }

    #[test]
    fn phpcs_report_should_map_errors_to_warnings() {
        let json = r#"{"totals":{},"files":{"/app/a.php":{"messages":[{"message":"Line too long","source":"Generic.Files.LineLength","type":"WARNING","line":7}]}}}"#;
        let f = Tool::Phpcs.parse(json, Path::new("/app/a.php")).unwrap();
        assert_eq!((f[0].line, f[0].severity), (7, Severity::Weak));
    }

    #[test]
    fn phpmd_report_should_name_the_rule() {
        let json = r#"{"files":[{"file":"/app/a.php","violations":[{"beginLine":9,"description":"Avoid unused local variables such as '$x'.","rule":"UnusedLocalVariable"}]}]}"#;
        let f = Tool::Phpmd.parse(json, Path::new("/app/a.php")).unwrap();
        assert_eq!(f[0].line, 9);
        assert!(f[0].message.ends_with("(UnusedLocalVariable)"));
    }

    #[test]
    fn pint_report_should_match_project_relative_paths() {
        let json = r#"{"tool":"pint","result":"fail","files":[{"path":"app\/Models\/User.php","fixers":["class_attributes_separation"]}]}"#;
        let f = Tool::Pint
            .parse(json, Path::new("/var/www/html/app/Models/User.php"))
            .unwrap();
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("class_attributes_separation"));
    }

    #[test]
    fn fixer_report_should_point_at_each_hunk() {
        let json = r#"{"files":[{"name":"/app/a.php","appliedFixers":["braces"],"diff":"--- a\n+++ b\n@@ -4,3 +4,3 @@\n-x\n+y\n@@ -20,2 +20,2 @@\n"}]}"#;
        let f = Tool::PhpCsFixer
            .parse(json, Path::new("/app/a.php"))
            .unwrap();
        assert_eq!(f.iter().map(|x| x.line).collect::<Vec<_>>(), [4, 20]);
    }
}
