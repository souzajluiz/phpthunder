//! Bottom-panel bodies and editor marks added with inspections: the
//! Problems list, the type hierarchy, and diagnostic squiggles.

use super::*;
use crate::app::{App, Message, TestTarget, Tests};
use crate::editor::Document;
use crate::testing::Outcome;
use crate::tr;
use iced::widget::{button, column, container, pin, row, scrollable, space};
use iced::{Center, Color, Element, Fill, Padding, Theme};
use phpthunder::inspection::Severity;

/// Squiggle color for a severity: red for errors, amber for warnings,
/// a quiet gray-blue for style hints.
pub(crate) fn severity_color(severity: Severity) -> Color {
    match severity {
        Severity::Error => rgb(0xF7768E),
        Severity::Warning => rgb(0xE0AF68),
        Severity::Weak => rgb(0x737AA2),
    }
}

impl App {
    /// The Problems panel: every diagnostic of the active file, in order.
    pub(crate) fn problems_body(&self) -> Element<'_, Message> {
        let Some(doc) = self.doc() else {
            return container(label(tr!("problems-no-file"), 12.0, MUTED))
                .padding([10, 16])
                .into();
        };
        let problems = doc.problems();
        if problems.is_empty() {
            return container(label(tr!("problems-none", file = doc.title()), 12.0, MUTED))
                .padding([10, 16])
                .into();
        }
        let rows = problems.into_iter().enumerate().map(|(i, d)| {
            let mark = if d.severity == Severity::Error {
                Icon::Error
            } else {
                Icon::Warning
            };
            button(
                row![
                    icon(mark, 14.0, 2.0, severity_color(d.severity)),
                    label(crate::i18n::diagnostic_message(d), 13.0, TEXT),
                    space().width(Fill),
                    label(
                        format!("{}:{}", doc.title(), d.span.start.line + 1),
                        12.0,
                        MUTED
                    ),
                ]
                .spacing(10)
                .align_y(Center),
            )
            .on_press(Message::OpenProblem(i))
            .width(Fill)
            .height(26)
            .padding([0, 8])
            .style(row_button(false))
            .into()
        });
        scrollable(column(rows).spacing(1).padding([4, 8]))
            .height(Fill)
            .style(scrollbars)
            .into()
    }

    /// The Hierarchy panel: supertypes indented down to the class, then
    /// its subtypes one level deeper.
    pub(crate) fn hierarchy_body(&self) -> Element<'_, Message> {
        let Some(h) = &self.hierarchy else {
            return space().into();
        };
        let rows = h.rows.iter().enumerate().map(|(i, r)| {
            let indent = space().width(r.depth as f32 * 18.0);
            let color = if r.target.is_some() { TEXT } else { MUTED };
            button(
                row![
                    indent,
                    icon(Icon::Hierarchy, 13.0, 2.0, ACCENT),
                    label(r.label.clone(), 13.0, color),
                    label(r.detail.clone(), 12.0, MUTED)
                ]
                .spacing(8)
                .align_y(Center),
            )
            .on_press_maybe(r.target.as_ref().map(|_| Message::OpenHierarchy(i)))
            .width(Fill)
            .height(26)
            .padding([0, 8])
            .style(row_button(false))
            .into()
        });
        column![
            container(label(h.title.clone(), 12.0, MUTED)).padding(Padding {
                top: 10.0,
                left: 16.0,
                right: 16.0,
                bottom: 4.0
            }),
            scrollable(column(rows).spacing(1).padding([2, 8]))
                .height(Fill)
                .style(scrollbars),
        ]
        .into()
    }

    /// Laravel markers at the left edge of the gutter on visible lines:
    /// a jump to a route's controller method or a model's migration, or a
    /// run button for migrations, seeders and Artisan commands. Shown only
    /// for the text they were computed on.
    pub(crate) fn laravel_gutter_marks<'a>(
        &'a self,
        doc: &'a Document,
    ) -> Vec<Element<'a, Message>> {
        use phpthunder::laravel::GutterKind;
        if doc.diagnostics_version != doc.version() {
            return Vec::new();
        }
        let layout = self.wrap_layout(doc);
        let (first, last) = self.visible_lines(doc, &layout);
        doc.gutter_markers
            .iter()
            .filter(|m| (first..=last).contains(&(m.line as usize)))
            .map(|m| {
                let (glyph, tip) = match m.kind {
                    GutterKind::RouteAction => (
                        Icon::Symbols,
                        tr!("gutter-route-action", target = m.detail.as_str()),
                    ),
                    GutterKind::ModelTable => (
                        Icon::Database,
                        tr!("gutter-model-table", table = m.detail.as_str()),
                    ),
                    GutterKind::RunMigration | GutterKind::RunSeeder | GutterKind::RunCommand => {
                        (Icon::Resume, tr!("gutter-run", command = m.detail.as_str()))
                    }
                };
                let color = match m.kind {
                    GutterKind::RouteAction | GutterKind::ModelTable => ACCENT,
                    _ => SYN_STRING,
                };
                let b = button(icon(glyph, 12.0, 2.0, color))
                    .padding(1)
                    .style(ghost_button)
                    .on_press(Message::Gutter(m.action.clone()));
                let tip = iced::widget::tooltip(
                    b,
                    container(label(tip, 12.0, TEXT))
                        .padding([4, 8])
                        .style(popup),
                    iced::widget::tooltip::Position::Right,
                );
                pin(tip)
                    .x(2.0)
                    .y(crate::app::row_top(&layout, m.line as usize, 0) + (CODE_LINE - 14.0) / 2.0)
                    .into()
            })
            .collect()
    }

    /// Squiggles under diagnostics on visible lines, drawn beneath the text.
    pub(crate) fn diagnostic_marks<'a>(&'a self, doc: &'a Document) -> Vec<Element<'a, Message>> {
        // Built-in results are drawn only for the text they were computed on;
        // external ones (line-based) are kept until the next save.
        let current = doc.diagnostics_version == doc.version();
        let layout = self.wrap_layout(doc);
        let (first_line, last_line) = self.visible_lines(doc, &layout);
        let (first_line, last_line) = (first_line as u32, last_line as u32);
        let shown: Vec<_> = if current {
            doc.diagnostics
                .iter()
                .chain(&doc.external_diagnostics)
                .collect()
        } else {
            doc.external_diagnostics.iter().collect()
        };
        shown
            .into_iter()
            .filter(|d| (first_line..=last_line).contains(&d.span.start.line))
            .filter(|d| d.end <= doc.text.len())
            .take(300)
            .map(|d| {
                let a = doc.position_of(d.start);
                let line_end = doc.line_end(a.line);
                let b = doc.position_of(d.end.min(line_end));
                let col = doc.visual_column(a);
                let x = crate::app::col_left(&layout, col);
                let w = (doc.visual_column(b).saturating_sub(col)).max(1) as f32 * CHAR_WIDTH;
                let w = if layout.wraps() {
                    w.min((self.wrap_columns() - layout.column_in_row(col)) as f32 * CHAR_WIDTH)
                } else {
                    w
                };
                let color = severity_color(d.severity);
                pin(container(space())
                    .width(w)
                    .height(2)
                    .style(move |_: &Theme| container::Style {
                        background: Some(color.into()),
                        border: iced::border::rounded(1),
                        ..Default::default()
                    }))
                .x(x)
                .y(crate::app::row_top(&layout, a.line, col) + CODE_LINE - 3.0)
                .into()
            })
            .collect()
    }

    /// Coverage of a run: overall statement coverage, a button hiding the
    /// editor marks, and the least covered files (click to open).
    fn coverage_summary<'a>(
        &'a self,
        coverage: &'a crate::testing::Coverage,
    ) -> Element<'a, Message> {
        let (covered, total) = coverage.values().fold((0, 0), |(c, t), f| {
            (c + f.covered.len(), t + f.covered.len() + f.uncovered.len())
        });
        #[expect(
            clippy::cast_precision_loss,
            reason = "line counts are far below f32 precision limits"
        )]
        let overall = if total > 0 {
            covered as f32 * 100.0 / total as f32
        } else {
            0.0
        };
        let mut files: Vec<(&std::path::PathBuf, f32)> = coverage
            .iter()
            .filter_map(|(p, f)| f.percent().map(|pc| (p, pc)))
            .collect();
        files.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut col = column![
            row![
                label(
                    tr!("tests-coverage", percent = format!("{overall:.1}")),
                    12.0,
                    TEXT
                ),
                space().width(Fill),
                button(label(tr!("tests-hide-coverage"), 12.0, TEXT_SOFT))
                    .padding([3, 10])
                    .style(ghost_button)
                    .on_press(Message::ClearCoverage),
            ]
            .align_y(Center)
        ]
        .spacing(1);
        for (path, percent) in files.into_iter().take(8) {
            col = col.push(
                button(row![
                    label(format!("{percent:>5.1}%"), 12.0, MUTED)
                        .font(MONO)
                        .width(60),
                    label(self.relative(path), 12.0, TEXT_SOFT),
                ])
                .on_press(Message::OpenFile(path.clone()))
                .width(Fill)
                .padding([2, 8])
                .style(row_button(false)),
            );
        }
        container(col).padding([4, 8]).into()
    }

    /// The Tests panel: a summary with run buttons, then every test case;
    /// failures show their message.
    pub(crate) fn tests_body(&self) -> Element<'_, Message> {
        let action = |name: String, target: TestTarget| -> Element<'_, Message> {
            button(label(name, 12.0, TEXT_SOFT))
                .padding([3, 10])
                .style(ghost_button)
                .on_press(Message::RunTests(target))
                .into()
        };
        let actions = row![
            action(tr!("tests-run-again"), TestTarget::Again),
            action(tr!("tests-run-file"), TestTarget::File),
            action(tr!("tests-run-all"), TestTarget::All),
            action(tr!("tests-run-coverage"), TestTarget::Coverage),
            button(label(tr!("debug-test"), 12.0, TEXT_SOFT))
                .padding([3, 10])
                .style(ghost_button)
                .on_press(Message::DebugTest),
        ]
        .spacing(4);
        let (summary, rows): (Element<'_, Message>, Vec<Element<'_, Message>>) = match &self.tests {
            None => (label(tr!("tests-none"), 12.0, MUTED).into(), Vec::new()),
            Some(Tests::Running(scope)) => (
                label(tr!("tests-running", scope = scope.as_str()), 12.0, MUTED).into(),
                Vec::new(),
            ),
            Some(Tests::Done(run)) => {
                let (passed, failed, skipped) = run.counts();
                let color = if failed > 0 {
                    severity_color(Severity::Error)
                } else {
                    rgb(0x9ECE6A)
                };
                let summary = row![
                    label(run.scope.label(), 12.0, TEXT),
                    label(tr!("tests-passed", count = passed), 12.0, rgb(0x9ECE6A)),
                    label(tr!("tests-failed", count = failed), 12.0, color),
                    label(tr!("tests-skipped", count = skipped), 12.0, MUTED),
                    label(format!("{:.2} s", run.elapsed.as_secs_f64()), 12.0, MUTED),
                ]
                .spacing(12);
                let mut rows: Vec<Element<'_, Message>> = Vec::new();
                if let Some(coverage) = &run.coverage {
                    rows.push(self.coverage_summary(coverage));
                }
                if run.cases.is_empty() {
                    // No report: show what the runner printed (fatal errors, config issues).
                    let tail: String = run
                        .output
                        .lines()
                        .rev()
                        .take(12)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("\n");
                    rows.push(
                        container(label(tail, 12.0, TEXT_SOFT).font(MONO))
                            .padding([4, 8])
                            .into(),
                    );
                }
                for (i, case) in run.cases.iter().enumerate() {
                    let (mark, color) = match &case.outcome {
                        Outcome::Passed => ("✓", rgb(0x9ECE6A)),
                        Outcome::Failed(_) => ("✕", severity_color(Severity::Error)),
                        Outcome::Skipped => ("–", MUTED),
                    };
                    let class = case
                        .class
                        .rsplit('\\')
                        .next()
                        .unwrap_or(&case.class)
                        .to_string();
                    rows.push(
                        button(
                            row![
                                label(mark, 13.0, color).width(14),
                                label(case.name.clone(), 13.0, TEXT),
                                label(class, 12.0, MUTED),
                                space().width(Fill),
                                label(format!("{:.0} ms", case.time * 1000.0), 12.0, MUTED),
                            ]
                            .spacing(10)
                            .align_y(Center),
                        )
                        .on_press_maybe(case.file.as_ref().map(|_| Message::OpenTest(i)))
                        .width(Fill)
                        .height(26)
                        .padding([0, 8])
                        .style(row_button(false))
                        .into(),
                    );
                    if let Outcome::Failed(message) = &case.outcome {
                        let short: String = message.lines().take(6).collect::<Vec<_>>().join("\n");
                        rows.push(
                            container(label(short, 12.0, rgb(0xF0B4BE)).font(MONO))
                                .padding(Padding {
                                    left: 32.0,
                                    top: 2.0,
                                    bottom: 6.0,
                                    right: 8.0,
                                })
                                .into(),
                        );
                    }
                }
                (summary.into(), rows)
            }
        };
        column![
            container(row![summary, space().width(Fill), actions].align_y(Center)).padding(
                Padding {
                    top: 6.0,
                    left: 16.0,
                    right: 12.0,
                    bottom: 4.0
                }
            ),
            scrollable(column(rows).spacing(1).padding([2, 8]))
                .height(Fill)
                .style(scrollbars),
        ]
        .into()
    }
}
