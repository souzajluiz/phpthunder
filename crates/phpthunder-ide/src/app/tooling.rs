//! Project tools: running tests, reformatting with the project's
//! formatter, quality tools on save, and Composer commands.

use super::*;
use crate::composer::composer_commands;
use crate::palette::{Item, Target};
use crate::testing::{Scope, TestRun, run_tests, test_at};
use crate::tr;
use iced::Task;
use phpthunder::inspection::{Code, Diagnostic};
use std::path::PathBuf;

/// The 1-based line of the Behat scenario around 0-based `line`.
fn scenario_line(text: &str, line: usize) -> Option<usize> {
    text.lines()
        .take(line + 1)
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim_start();
            t.starts_with("Scenario") || t.starts_with("Example:")
        })
        .last()
        .map(|(i, _)| i + 1)
}

impl App {
    /// The scope a [`TestTarget`] means right now.
    pub(crate) fn test_scope(&self, target: TestTarget) -> Scope {
        let file = self.doc().filter(|d| d.is_php()).map(|d| d.path.clone());
        let is_test_file = file.as_ref().is_some_and(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            p.components()
                .any(|c| matches!(c.as_os_str().to_str(), Some("tests" | "spec" | "features")))
                || name.ends_with("Test.php")
                || name.ends_with("Cest.php")
                || name.ends_with("Spec.php")
                || name.ends_with(".feature")
                || name.ends_with(".phpt")
        });
        match target {
            TestTarget::All => Scope::All,
            TestTarget::Again | TestTarget::Coverage => {
                self.last_test_scope.clone().unwrap_or(Scope::All)
            }
            TestTarget::File => match file.filter(|_| is_test_file) {
                Some(path) => Scope::File(path),
                None => Scope::All,
            },
            TestTarget::Context => match (file, self.doc()) {
                (Some(path), Some(doc)) if is_test_file && doc.title().ends_with(".feature") => {
                    match scenario_line(&doc.text, doc.cursor().line) {
                        Some(line) => Scope::Filter(path, line.to_string()),
                        None => Scope::File(path),
                    }
                }
                (Some(path), Some(doc)) if is_test_file => {
                    match test_at(&doc.text, doc.offset(doc.cursor())) {
                        Some(name) => Scope::Filter(path, name),
                        None => Scope::File(path),
                    }
                }
                _ => Scope::All,
            },
        }
    }

    /// Starts a test run in the background and shows the Tests panel.
    /// Unsaved test files are saved first so the run sees the edits.
    pub(crate) fn run_tests(&mut self, target: TestTarget) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        if matches!(self.tests, Some(Tests::Running(_))) {
            return Task::none();
        }
        self.save_all_php();
        let scope = self.test_scope(target);
        self.last_test_scope = Some(scope.clone());
        self.tests = Some(Tests::Running(scope.label()));
        self.show_panel(Bottom::Tests);
        let coverage = target == TestTarget::Coverage;
        Task::perform(run_tests(root, scope, coverage), Message::TestsDone)
    }

    /// Stores a finished run; a failed start becomes a notice.
    pub(crate) fn tests_done(&mut self, result: Result<TestRun, String>) {
        match result {
            Ok(run) => {
                if let Some(coverage) = &run.coverage {
                    self.coverage = coverage.clone();
                }
                let (passed, failed, _) = run.counts();
                self.notify_background(
                    tr!("notify-tests"),
                    tr!("notify-tests-body", passed = passed, failed = failed),
                );
                self.tests = Some(Tests::Done(run));
            }
            Err(error) => {
                self.tests = None;
                self.fail(error);
            }
        }
    }

    /// Opens the declaration of the `i`-th test case of the last run.
    pub(crate) fn open_test(&mut self, i: usize) -> Task<Message> {
        let Some(Tests::Done(run)) = &self.tests else {
            return Task::none();
        };
        let Some(case) = run.cases.get(i) else {
            return Task::none();
        };
        let Some(path) = case.file.clone() else {
            return Task::none();
        };
        let line = case.line.unwrap_or(1).saturating_sub(1);
        self.record_nav();
        self.open(path, Some(phpthunder::syntax::Pos { line, col: 0 }))
    }

    /// Writes every modified PHP tab to disk (before running tests).
    pub(crate) fn save_all_php(&mut self) {
        let mut failures = Vec::new();
        for doc in self.docs.iter_mut().filter(|d| d.dirty && d.is_php()) {
            match std::fs::write(&doc.path, &doc.text) {
                Ok(()) => doc.mark_saved(),
                Err(e) => failures.push(format!("{}: {e}", doc.title())),
            }
        }
        if !failures.is_empty() {
            self.fail(tr!("notice-save-failed-many", files = failures.join(", ")));
        }
    }

    /// Reformats the active file (`⌥⌘L`) with the formatter for its type:
    /// the project's own tool, a built-in one, or whitespace clean-up per
    /// EditorConfig (see [`crate::formatter::format_file`]).
    pub(crate) fn reformat(&mut self) -> Task<Message> {
        let (Some(root), Some(doc)) = (self.tree.root.clone(), self.doc()) else {
            return Task::none();
        };
        let (path, version, text) = (doc.path.clone(), doc.version(), doc.text.clone());
        let file = path.clone();
        Task::perform(
            crate::formatter::format_file(root, file, text),
            move |result| Message::Formatted(path.clone(), version, result.map(Some)),
        )
    }

    /// Applies formatter output as one undo step, if the document did not
    /// change meanwhile. Without a formatter, trims trailing whitespace.
    pub(crate) fn formatted(
        &mut self,
        path: &std::path::Path,
        version: u64,
        result: Result<Option<(String, crate::formatter::Formatter)>, String>,
    ) {
        let Some(i) = self
            .docs
            .iter()
            .position(|d| d.path == path && d.version() == version)
        else {
            return;
        };
        let (text, notice) = match result {
            Ok(Some((text, crate::formatter::Formatter::Whitespace))) => {
                (text, tr!("notice-whitespace-trimmed"))
            }
            Ok(Some((text, formatter))) => {
                (text, tr!("notice-formatted", formatter = formatter.name()))
            }
            Ok(None) => {
                let trimmed: String = self.docs[i]
                    .text
                    .lines()
                    .map(str::trim_end)
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n";
                (trimmed, tr!("notice-whitespace-trimmed"))
            }
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        if text != self.docs[i].text {
            let cursor = self.docs[i].cursor();
            self.docs[i].set_text(text, cursor);
            if self.active == Some(i) {
                self.after_change(true);
            }
        }
        self.notice = Some(notice);
    }

    /// Starts every enabled quality tool (see [`crate::quality`]) on a
    /// just-saved PHP file; saving the project settings file re-reads the
    /// interpreter instead.
    pub(crate) fn quality_after_save(&mut self, path: PathBuf) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        if path.ends_with(crate::interpreter::PROJECT_FILE) {
            self.refresh_interpreter();
            return Task::none();
        }
        if path.file_name().is_some_and(|n| n == "composer.json") {
            self.refresh_composer_problems();
            return Task::none();
        }
        let tools: Vec<_> = crate::quality::enabled_tools(&root)
            .into_iter()
            .filter(|t| t.applies_to(&path))
            .collect();
        self.quality_running += tools.len();
        Task::batch(tools.into_iter().map(|tool| {
            let file = path.clone();
            let reported = path.clone();
            Task::perform(
                crate::quality::analyse(tool, root.clone(), file),
                move |result| Message::QualityDone(tool, reported, result),
            )
        }))
    }

    /// Replaces the problems `tool` reported earlier on the document with
    /// its new findings, spanning the reported lines.
    pub(crate) fn quality_done(
        &mut self,
        tool: crate::quality::Tool,
        path: &std::path::Path,
        result: Result<Vec<crate::quality::Finding>, String>,
    ) {
        self.quality_running = self.quality_running.saturating_sub(1);
        let findings = match result {
            Ok(f) => f,
            Err(error) => {
                self.fail(format!("{}: {error}", tool.name()));
                return;
            }
        };
        let Some(doc) = self.docs.iter_mut().find(|d| d.path == path) else {
            return;
        };
        let line_count = doc.line_count();
        let name = tool.name();
        doc.external_diagnostics
            .retain(|d| !matches!(&d.code, Code::External(t) if t == name));
        for f in findings {
            let line = (f.line.saturating_sub(1) as usize).min(line_count.saturating_sub(1));
            let text = doc.line_text(line);
            let indent = text.len() - text.trim_start().len();
            let (start, end) = (doc.line_start(line) + indent, doc.line_end(line));
            let span = phpthunder::syntax::Span {
                start: doc.utf16_pos(doc.position_of(start)),
                end: doc.utf16_pos(doc.position_of(end)),
            };
            doc.external_diagnostics.push(Diagnostic {
                span,
                start,
                end,
                severity: f.severity,
                message: format!("{name}: {}", f.message),
                code: Code::External(name.into()),
            });
        }
    }

    /// Recomputes the problems of the active `composer.json` (packages not
    /// installed, required twice); other documents are left alone.
    pub(crate) fn refresh_composer_problems(&mut self) {
        let Some(root) = self.tree.root.clone() else {
            return;
        };
        let Some(doc) = self.doc_mut() else {
            return;
        };
        if doc.path != root.join("composer.json") {
            return;
        }
        let problems = crate::composer::json_problems(&root, &doc.text);
        doc.external_diagnostics
            .retain(|d| !matches!(&d.code, Code::External(t) if t == "Composer"));
        for p in problems {
            let span = phpthunder::syntax::Span {
                start: doc.utf16_pos(doc.position_of(p.start)),
                end: doc.utf16_pos(doc.position_of(p.end)),
            };
            let (severity, message) = match p.kind {
                crate::composer::JsonProblemKind::NotInstalled => (
                    phpthunder::inspection::Severity::Warning,
                    tr!("composer-not-installed", package = p.package.as_str()),
                ),
                crate::composer::JsonProblemKind::Duplicated => (
                    phpthunder::inspection::Severity::Warning,
                    tr!("composer-duplicated", package = p.package.as_str()),
                ),
            };
            doc.external_diagnostics.push(Diagnostic {
                span,
                start: p.start,
                end: p.end,
                severity,
                message,
                code: Code::External("Composer".into()),
            });
        }
    }

    /// Opens a picker with the project's Composer scripts and common
    /// commands; the choice runs in the terminal (`⌃⌥C`).
    pub(crate) fn open_composer(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let interpreter = crate::interpreter::Interpreter::of(&root);
        let items: Vec<Item> = composer_commands(&root)
            .into_iter()
            .map(|c| {
                let args: Vec<String> = c.args.split_whitespace().map(str::to_string).collect();
                let line = interpreter.shell_line(&root, "composer", &args);
                Item {
                    badge: "›",
                    title: format!("composer {}", c.args.trim_end()),
                    matched: Vec::new(),
                    target: if c.is_draft() {
                        Target::TerminalDraft(format!("{line} "))
                    } else {
                        Target::Command(line)
                    },
                    detail: c.description,
                }
            })
            .collect();
        if items.is_empty() {
            self.notice = Some(tr!("notice-no-composer"));
            return Task::none();
        }
        self.open_choice(tr!("composer-title"), items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_line_should_find_the_enclosing_scenario() {
        let feature = "Feature: Cart\n\n  Scenario: Add\n    Given a cart\n\n  Scenario: Remove\n    When I remove\n";
        assert_eq!(scenario_line(feature, 3), Some(3));
        assert_eq!(scenario_line(feature, 6), Some(6));
        assert_eq!(scenario_line(feature, 0), None);
    }
}
