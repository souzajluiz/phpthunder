//! Running inspections in the background and showing their results.

use super::*;
use iced::Task;
use phpthunder::inspection::{Diagnostic, Severity};
use std::path::Path;
use std::time::Duration;

/// Quiet period after the last edit before inspections run, so typing is
/// never slowed down by analysis.
const INSPECTION_DELAY: Duration = Duration::from_millis(350);

impl App {
    /// Turns a pending inspection request into a debounce timer. Called by
    /// [`App::update`] after every message.
    pub(crate) fn schedule_inspection(&mut self) -> Task<Message> {
        if !std::mem::take(&mut self.inspect_pending) {
            return Task::none();
        }
        self.inspect_generation += 1;
        let generation = self.inspect_generation;
        Task::perform(tokio::time::sleep(INSPECTION_DELAY), move |_| {
            Message::Inspect(generation)
        })
    }

    /// Inspects the active PHP document on a blocking thread when the
    /// debounce timer `generation` is still the latest one.
    pub(crate) fn run_inspection(&mut self, generation: u64) -> Task<Message> {
        if generation != self.inspect_generation {
            return Task::none();
        }
        let Some(doc) = self.doc().filter(|d| d.is_php()) else {
            return Task::none();
        };
        let (path, version, text, lang) = (
            doc.path.clone(),
            doc.version(),
            doc.text.clone(),
            self.lang.clone(),
        );
        Task::perform(
            async move {
                let diagnostics = tokio::task::spawn_blocking(move || lang.diagnostics(&text))
                    .await
                    .unwrap_or_default();
                (path, version, diagnostics)
            },
            |(path, version, diagnostics)| Message::Inspected(path, version, diagnostics),
        )
    }

    /// Stores inspection results on their document, unless it changed
    /// since they were computed (a newer run is already scheduled then).
    pub(crate) fn store_diagnostics(
        &mut self,
        path: &Path,
        version: u64,
        diagnostics: Vec<Diagnostic>,
    ) {
        if let Some(doc) = self
            .docs
            .iter_mut()
            .find(|d| d.path == path && d.version() == version)
        {
            doc.diagnostics = diagnostics;
            doc.diagnostics_version = version;
        }
    }

    /// Error and warning counts of the active document, for the status bar.
    pub fn problem_counts(&self) -> (usize, usize) {
        let Some(doc) = self.doc() else { return (0, 0) };
        let problems = doc.problems();
        let errors = problems
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        (errors, problems.len() - errors)
    }

    /// The diagnostics touching byte `offset` of the active document.
    pub(crate) fn diagnostics_at(&self, offset: usize) -> Vec<Diagnostic> {
        self.doc()
            .map(|d| {
                d.problems()
                    .into_iter()
                    .filter(|x| x.start <= offset && offset <= x.end)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Jumps to the `i`-th problem of the active document (Problems panel).
    pub(crate) fn open_problem(&mut self, i: usize) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let Some(d) = doc.problems().get(i).copied() else {
            return Task::none();
        };
        let (path, pos) = (doc.path.clone(), d.span.start);
        self.record_nav();
        self.open(path, Some(pos))
    }
}
