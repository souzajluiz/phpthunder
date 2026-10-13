//! Bottom-panel bodies and editor marks added with inspections: the
//! Problems list, the type hierarchy, and diagnostic squiggles.

use super::*;
use crate::app::{App, Message, TestTarget, Tests};
use crate::editor::Document;
use crate::testing::Outcome;
use iced::widget::{button, column, container, pin, row, scrollable, space};
use iced::{Center, Color, Element, Fill, Padding, Theme};
use phpthunder::inspection::Severity;

/// Squiggle color for a severity: red for errors, amber for warnings.
pub(crate) fn severity_color(severity: Severity) -> Color {
    match severity {
        Severity::Error => rgb(0xF7768E),
        Severity::Warning => rgb(0xE0AF68),
    }
}

impl App {
    /// The Problems panel: every diagnostic of the active file, in order.
    pub(crate) fn problems_body(&self) -> Element<'_, Message> {
        let Some(doc) = self.doc() else {
            return container(label("No file open", 12.0, MUTED))
                .padding([10, 16])
                .into();
        };
        let problems = doc.problems();
        if problems.is_empty() {
            return container(label(
                format!("No problems in {}", doc.title()),
                12.0,
                MUTED,
            ))
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
                    label(d.message.clone(), 13.0, TEXT),
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

    /// Squiggles under diagnostics on visible lines, drawn beneath the text.
    pub(crate) fn diagnostic_marks<'a>(&'a self, doc: &'a Document) -> Vec<Element<'a, Message>> {
        // Built-in results are drawn only for the text they were computed on;
        // external ones (line-based) are kept until the next save.
        let current = doc.diagnostics_version == doc.version();
        let first_line = ((doc.scroll_y - EDITOR_PAD_TOP) / CODE_LINE)
            .floor()
            .max(0.0) as u32;
        let last_line = first_line + (self.viewport_height / CODE_LINE).ceil() as u32 + 1;
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
                let x = GUTTER_WIDTH + EDITOR_PAD_LEFT + doc.visual_column(a) as f32 * CHAR_WIDTH;
                let w = (doc.visual_column(b).saturating_sub(doc.visual_column(a))).max(1) as f32
                    * CHAR_WIDTH;
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
                .y(EDITOR_PAD_TOP + (a.line + 1) as f32 * CODE_LINE - 3.0)
                .into()
            })
            .collect()
    }

    /// The Tests panel: a summary with run buttons, then every test case;
    /// failures show their message.
    pub(crate) fn tests_body(&self) -> Element<'_, Message> {
        let action = |name: &'static str, target: TestTarget| -> Element<'_, Message> {
            button(label(name, 12.0, TEXT_SOFT))
                .padding([3, 10])
                .style(ghost_button)
                .on_press(Message::RunTests(target))
                .into()
        };
        let actions = row![
            action("Run again", TestTarget::Again),
            action("Run file", TestTarget::File),
            action("Run all", TestTarget::All),
        ]
        .spacing(4);
        let (summary, rows): (Element<'_, Message>, Vec<Element<'_, Message>>) = match &self.tests {
            None => (label("No test run yet", 12.0, MUTED).into(), Vec::new()),
            Some(Tests::Running(scope)) => (
                label(format!("Running {scope}…"), 12.0, MUTED).into(),
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
                    label(format!("{passed} passed"), 12.0, rgb(0x9ECE6A)),
                    label(format!("{failed} failed"), 12.0, color),
                    label(format!("{skipped} skipped"), 12.0, MUTED),
                    label(format!("{:.2} s", run.elapsed.as_secs_f64()), 12.0, MUTED),
                ]
                .spacing(12);
                let mut rows: Vec<Element<'_, Message>> = Vec::new();
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
