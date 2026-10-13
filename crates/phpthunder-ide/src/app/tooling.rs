//! Project tools: running tests, reformatting with the project's
//! formatter, PHPStan on save, and Composer commands.

use super::*;
use crate::composer::composer_commands;
use crate::palette::{Item, Target};
use crate::testing::{Scope, TestRun, run_tests, test_at};
use crate::tr;
use iced::Task;
use phpthunder::inspection::{Code, Diagnostic, Severity};
use std::path::PathBuf;

impl App {
    /// The scope a [`TestTarget`] means right now.
    pub(crate) fn test_scope(&self, target: TestTarget) -> Scope {
        let file = self.doc().filter(|d| d.is_php()).map(|d| d.path.clone());
        let is_test_file = file.as_ref().is_some_and(|p| {
            p.components().any(|c| c.as_os_str() == "tests")
                || p.file_stem()
                    .is_some_and(|s| s.to_string_lossy().ends_with("Test"))
        });
        match target {
            TestTarget::All => Scope::All,
            TestTarget::Again => self.last_test_scope.clone().unwrap_or(Scope::All),
            TestTarget::File => match file.filter(|_| is_test_file) {
                Some(path) => Scope::File(path),
                None => Scope::All,
            },
            TestTarget::Context => match (file, self.doc()) {
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
        Task::perform(run_tests(root, scope), Message::TestsDone)
    }

    /// Stores a finished run; a failed start becomes a notice.
    pub(crate) fn tests_done(&mut self, result: Result<TestRun, String>) {
        match result {
            Ok(run) => self.tests = Some(Tests::Done(run)),
            Err(error) => {
                self.tests = None;
                self.notice = Some(error);
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
            self.notice = Some(tr!("notice-save-failed-many", files = failures.join(", ")));
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
                self.notice = Some(error);
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

    /// Starts PHPStan on a just-saved PHP file when the project uses it.
    pub(crate) fn phpstan_after_save(&mut self, path: PathBuf) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        if !crate::phpstan::phpstan_available(&root) || path.extension().is_none_or(|e| e != "php")
        {
            return Task::none();
        }
        self.phpstan_running = true;
        let file = path.clone();
        Task::perform(crate::phpstan::analyse(root, file), move |result| {
            Message::PhpstanDone(path, result)
        })
    }

    /// Stores PHPStan findings on their document as external diagnostics
    /// spanning the reported lines.
    pub(crate) fn phpstan_done(
        &mut self,
        path: &std::path::Path,
        result: Result<Vec<crate::phpstan::Finding>, String>,
    ) {
        self.phpstan_running = false;
        let findings = match result {
            Ok(f) => f,
            Err(error) => {
                self.notice = Some(format!("PHPStan: {error}"));
                return;
            }
        };
        let Some(doc) = self.docs.iter_mut().find(|d| d.path == path) else {
            return;
        };
        let line_count = doc.line_count();
        doc.external_diagnostics = findings
            .into_iter()
            .map(|f| {
                let line = (f.line.saturating_sub(1) as usize).min(line_count.saturating_sub(1));
                let text = doc.line_text(line);
                let indent = text.len() - text.trim_start().len();
                let (start, end) = (doc.line_start(line) + indent, doc.line_end(line));
                let span = phpthunder::syntax::Span {
                    start: doc.utf16_pos(doc.position_of(start)),
                    end: doc.utf16_pos(doc.position_of(end)),
                };
                Diagnostic {
                    span,
                    start,
                    end,
                    severity: Severity::Error,
                    message: format!("PHPStan: {}", f.message),
                    code: Code::External("PHPStan".into()),
                }
            })
            .collect();
    }

    /// Opens a picker with the project's Composer scripts and common
    /// commands; the choice runs in the terminal (`⌃⌥C`).
    pub(crate) fn open_composer(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let items: Vec<Item> = composer_commands(&root)
            .into_iter()
            .map(|c| Item {
                badge: "›",
                title: format!("composer {}", c.args),
                matched: Vec::new(),
                detail: c.description,
                target: Target::Command(format!("composer {}", c.args)),
            })
            .collect();
        if items.is_empty() {
            self.notice = Some(tr!("notice-no-composer"));
            return Task::none();
        }
        self.open_choice(tr!("composer-title"), items)
    }
}
