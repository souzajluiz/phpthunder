//! Running the project's tests (Pest, PHPUnit, Codeception, Behat or
//! PHPSpec) through the project's interpreter, and collecting results and
//! coverage.

use super::*;
use crate::interpreter::Interpreter;
use crate::project::vendor_bin;
use crate::tr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A test framework the IDE can run from `vendor/bin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framework {
    /// Pest (preferred when both Pest and PHPUnit are installed).
    Pest,
    /// PHPUnit (also runs `.phpt` files).
    PhpUnit,
    /// Codeception (`*Cest.php`, `*Cept.php`).
    Codeception,
    /// Behat (`.feature` files).
    Behat,
    /// PHPSpec (`spec/**/*Spec.php`).
    PhpSpec,
}

impl Framework {
    /// The `vendor/bin` script.
    fn bin(self) -> &'static str {
        match self {
            Framework::Pest => "pest",
            Framework::PhpUnit => "phpunit",
            Framework::Codeception => "codecept",
            Framework::Behat => "behat",
            Framework::PhpSpec => "phpspec",
        }
    }

    /// Whether the framework writes a Clover coverage report itself; the
    /// others get coverage from a prepended collector script.
    pub fn writes_clover(self) -> bool {
        matches!(
            self,
            Framework::Pest | Framework::PhpUnit | Framework::Codeception
        )
    }
}

/// Scope of a test run.
#[derive(Debug, Clone)]
pub enum Scope {
    /// The whole suite.
    All,
    /// One test file.
    File(PathBuf),
    /// Tests in a file whose name matches a filter (a method, a Pest
    /// description, or the line of a Behat scenario).
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
            Scope::All => tr!("tests-all"),
            Scope::File(p) => name(p),
            Scope::Filter(p, f) => format!("{} › {f}", name(p)),
        }
    }

    /// The file the scope is about, if any.
    fn file(&self) -> Option<&Path> {
        match self {
            Scope::All => None,
            Scope::File(p) | Scope::Filter(p, _) => Some(p),
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
    /// Line coverage per file (project paths), for runs with coverage.
    pub coverage: Option<Coverage>,
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

/// The project's main test runner (Pest, else PHPUnit, Codeception,
/// PHPSpec, Behat), if any; used for whole-suite runs and debugging.
pub fn detect_framework(root: &Path) -> Option<(Framework, PathBuf)> {
    [
        Framework::Pest,
        Framework::PhpUnit,
        Framework::Codeception,
        Framework::PhpSpec,
        Framework::Behat,
    ]
    .into_iter()
    .find_map(|f| vendor_bin(root, f.bin()).map(|bin| (f, bin)))
}

/// The runner for `scope`: by file kind (`.feature`, Codeception and
/// PHPSpec naming, `.phpt`), else the project's main one.
pub fn framework_for(root: &Path, scope: &Scope) -> Option<(Framework, PathBuf)> {
    let wanted = scope.file().and_then(|file| {
        let name = file.file_name()?.to_string_lossy().into_owned();
        let in_spec = file
            .strip_prefix(root)
            .is_ok_and(|rel| rel.starts_with("spec"));
        if name.ends_with(".feature") {
            Some(Framework::Behat)
        } else if name.ends_with("Cest.php") || name.ends_with("Cept.php") {
            Some(Framework::Codeception)
        } else if in_spec && name.ends_with("Spec.php") {
            Some(Framework::PhpSpec)
        } else if name.ends_with(".phpt") {
            Some(Framework::PhpUnit)
        } else {
            None
        }
    });
    match wanted {
        Some(f) => vendor_bin(root, f.bin()).map(|bin| (f, bin)),
        None => detect_framework(root),
    }
}

/// The line of `function name` in `file` (PHPSpec runs examples by line).
fn method_line(file: &Path, name: &str) -> Option<usize> {
    let text = std::fs::read_to_string(file).ok()?;
    let needle = format!("function {name}");
    text.lines()
        .position(|l| l.contains(&needle))
        .map(|i| i + 1)
}

/// Arguments after the script for running `scope` with `framework`,
/// writing its JUnit report to `report` (Behat: a folder) and, with
/// `clover`, its coverage there. `remote` translates a project path for
/// the interpreter, for paths embedded in larger arguments.
fn framework_arguments(
    framework: Framework,
    scope: &Scope,
    report: &Path,
    clover: Option<&Path>,
    remote: &dyn Fn(&Path) -> String,
) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    let target = |sep: &str, filter: Option<String>| -> Vec<std::ffi::OsString> {
        match (scope.file(), filter) {
            (Some(file), Some(f)) => vec![format!("{}{sep}{f}", remote(file)).into()],
            (Some(file), None) => vec![file.as_os_str().to_owned()],
            (None, _) => Vec::new(),
        }
    };
    let filter = match scope {
        Scope::Filter(_, f) => Some(f.clone()),
        _ => None,
    };
    match framework {
        Framework::Pest | Framework::PhpUnit => {
            args.extend(["--colors=never".into(), "--log-junit".into(), report.into()]);
            if let Some(clover) = clover {
                args.extend(["--coverage-clover".into(), clover.into()]);
            }
            args.extend(scope_arguments(scope));
        }
        Framework::Codeception => {
            let out = report.parent().unwrap_or(report);
            args.extend([
                "run".into(),
                "--no-colors".into(),
                "--no-interaction".into(),
                "-o".into(),
                format!("paths: output: {}", remote(out)).into(),
                "--xml".into(),
                report.file_name().unwrap_or_default().to_owned(),
            ]);
            if let Some(clover) = clover {
                args.extend([
                    "--coverage".into(),
                    "--coverage-xml".into(),
                    clover.file_name().unwrap_or_default().to_owned(),
                ]);
            }
            args.extend(target(":", filter));
        }
        Framework::Behat => {
            args.extend([
                "--no-colors".into(),
                "--format=junit".into(),
                format!("--out={}", remote(report)).into(),
            ]);
            args.extend(target(":", filter));
        }
        Framework::PhpSpec => {
            args.extend([
                "run".into(),
                "--no-ansi".into(),
                "--no-interaction".into(),
                "--format=junit".into(),
            ]);
            let line = scope
                .file()
                .zip(filter)
                .and_then(|(file, f)| method_line(file, &f))
                .map(|l| l.to_string());
            args.extend(target(":", line));
        }
    }
    args
}

/// Runs `scope` with the right test framework (see [`framework_for`]) and
/// parses its JUnit report; with `coverage`, collects line coverage too
/// (Xdebug in coverage mode or PCOV): a Clover report for PHPUnit, Pest and
/// Codeception, a prepended collector script for Behat and PHPSpec. Runs
/// asynchronously.
///
/// # Errors
/// Returns a message when no framework is installed or the process cannot
/// start. A run with failing tests is a successful `TestRun`.
pub async fn run_tests(root: PathBuf, scope: Scope, coverage: bool) -> Result<TestRun, String> {
    let (framework, bin) =
        framework_for(&root, &scope).ok_or_else(|| tr!("error-no-test-runner"))?;
    let interpreter = Interpreter::of(&root);
    let scratch = interpreter
        .scratch_dir(&root)
        .join(format!("tests-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let _ = std::fs::create_dir_all(&scratch);
    let report = if framework == Framework::Behat {
        scratch.join("behat")
    } else {
        scratch.join("junit.xml")
    };
    let clover = (coverage && framework.writes_clover()).then(|| scratch.join("clover.xml"));
    // Behat and PHPSpec: a prepended script collects line coverage.
    let lines = (coverage && !framework.writes_clover()).then(|| scratch.join("coverage.json"));
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    if coverage {
        // Herd's Xdebug when PHP loads none (PCOV projects need none).
        if let Ok(xdebug) = crate::debug::xdebug_arguments(&root, &interpreter).await {
            args.extend(xdebug.into_iter().map(Into::into));
        }
        args.push("-dxdebug.mode=coverage".into());
        args.push(format!("-dpcov.directory={}", interpreter.to_remote(&root, &root)).into());
    }
    if let Some(out) = &lines {
        let script = scratch.join("coverage-prepend.php");
        let source = prepend_script(
            &interpreter.to_remote(&root, &root),
            &interpreter.to_remote(&root, out),
        );
        std::fs::write(&script, source).map_err(|e| e.to_string())?;
        args.push(
            format!(
                "-dauto_prepend_file={}",
                interpreter.to_remote(&root, &script)
            )
            .into(),
        );
    }
    args.push(bin.into_os_string());
    let remote = |p: &Path| interpreter.to_remote(&root, p);
    args.extend(framework_arguments(
        framework,
        &scope,
        &report,
        clover.as_deref(),
        &remote,
    ));
    let mut cmd = interpreter.command(&root, "php", &args);
    let started = Instant::now();
    let output = cmd
        .output()
        .await
        .map_err(|e| tr!("error-run-php", error = e.to_string()))?;
    let elapsed = started.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let console = format!("{stdout}{}", String::from_utf8_lossy(&output.stderr));
    let xml = match framework {
        Framework::PhpSpec => stdout,
        Framework::Behat => {
            let mut all = String::new();
            for entry in std::fs::read_dir(&report).into_iter().flatten().flatten() {
                if let Ok(text) = interpreter.read_back(&root, &entry.path()).await {
                    all.push_str(&text);
                }
            }
            all
        }
        _ => interpreter
            .read_back(&root, &report)
            .await
            .unwrap_or_default(),
    };
    let mut cases = if xml.contains("<testcase") {
        parse_junit(&xml[xml.find('<').unwrap_or(0)..])?
    } else {
        Vec::new()
    };
    for case in &mut cases {
        if let Some(file) = &case.file {
            case.file = Some(interpreter.to_local(&root, &file.to_string_lossy()));
        }
    }
    let parsed = match (&clover, &lines) {
        (Some(path), _) => {
            let xml = interpreter.read_back(&root, path).await.unwrap_or_default();
            Some(parse_clover(&xml).unwrap_or_default())
        }
        (None, Some(path)) => {
            let json = interpreter.read_back(&root, path).await.unwrap_or_default();
            Some(parse_line_json(&json).unwrap_or_default())
        }
        _ => None,
    };
    let coverage = parsed.map(|c| {
        c.into_iter()
            .map(|(p, c)| (interpreter.to_local(&root, &p.to_string_lossy()), c))
            .collect()
    });
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(TestRun {
        scope,
        cases,
        elapsed,
        output: console,
        coverage,
    })
}

/// Command-line arguments that select `scope` for Pest or PHPUnit: the
/// file, plus `--filter` with the test name escaped as a regex. Shared by
/// test runs and debug sessions.
pub fn scope_arguments(scope: &Scope) -> Vec<std::ffi::OsString> {
    match scope {
        Scope::All => Vec::new(),
        Scope::File(path) => vec![path.clone().into_os_string()],
        Scope::Filter(path, filter) => vec![
            path.clone().into_os_string(),
            "--filter".into(),
            regex::escape(filter).into(),
        ],
    }
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
