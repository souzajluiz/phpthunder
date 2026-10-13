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

    /// Inspects the active PHP or Blade document on a blocking thread when
    /// the debounce timer `generation` is still the latest one: engine
    /// inspections for PHP, unknown Laravel keys for both.
    pub(crate) fn run_inspection(&mut self, generation: u64) -> Task<Message> {
        if generation != self.inspect_generation {
            return Task::none();
        }
        let Some(doc) = self
            .doc()
            .filter(|d| d.is_php() || d.file_type == crate::filetype::FileType::Blade)
        else {
            return Task::none();
        };
        let (path, version, text, lang, php) = (
            doc.path.clone(),
            doc.version(),
            doc.text.clone(),
            self.lang.clone(),
            doc.is_php(),
        );
        let laravel = self.laravel.clone();
        let rel = self
            .tree
            .root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .map(|r| r.to_string_lossy().replace('\\', "/"));
        Task::perform(
            async move {
                let (diagnostics, markers) = tokio::task::spawn_blocking(move || {
                    // Blade templates get only the Laravel key checks: the
                    // PHP inspections would misread their markup.
                    let mut found = if php {
                        lang.diagnostics(&text)
                    } else {
                        Vec::new()
                    };
                    let mut markers = Vec::new();
                    if let Some(project) = laravel {
                        found.extend(phpthunder::laravel::key_diagnostics(&project, &text));
                        if php {
                            found.extend(phpthunder::laravel::order_by_diagnostics(&text));
                            if let Some(rel) = rel {
                                markers = lang.gutter_markers(&text, &rel);
                            }
                        }
                    }
                    (found, markers)
                })
                .await
                .unwrap_or_default();
                (path, version, diagnostics, markers)
            },
            |(path, version, diagnostics, markers)| {
                Message::Inspected(path, version, diagnostics, markers)
            },
        )
    }

    /// Stores inspection results and Laravel gutter markers on their
    /// document, unless it changed since they were computed (a newer run is
    /// already scheduled then).
    pub(crate) fn store_diagnostics(
        &mut self,
        path: &Path,
        version: u64,
        diagnostics: Vec<Diagnostic>,
        markers: Vec<phpthunder::laravel::GutterMarker>,
    ) {
        if let Some(doc) = self
            .docs
            .iter_mut()
            .find(|d| d.path == path && d.version() == version)
        {
            doc.diagnostics = diagnostics;
            doc.gutter_markers = markers;
            doc.diagnostics_version = version;
        }
    }

    /// Follows a clicked Laravel gutter marker: opens its target, or runs
    /// its command in the terminal.
    pub(crate) fn gutter_action(
        &mut self,
        action: phpthunder::laravel::GutterAction,
    ) -> Task<Message> {
        match action {
            phpthunder::laravel::GutterAction::Open { path, pos } => {
                self.record_nav();
                self.open(path, Some(pos))
            }
            phpthunder::laravel::GutterAction::Run(command) => self.run_in_terminal(&command),
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
