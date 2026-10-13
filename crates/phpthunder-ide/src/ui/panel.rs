//! Bottom-panel bodies and editor marks added with inspections: the
//! Problems list, the type hierarchy, and diagnostic squiggles.

use super::*;
use crate::app::{App, Message};
use crate::editor::Document;
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
        if doc.diagnostics.is_empty() {
            return container(label(
                format!("No problems in {}", doc.title()),
                12.0,
                MUTED,
            ))
            .padding([10, 16])
            .into();
        }
        let rows = doc.diagnostics.iter().enumerate().map(|(i, d)| {
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
        if doc.diagnostics.is_empty() || doc.diagnostics_version != doc.version() {
            return Vec::new();
        }
        let first_line = ((doc.scroll_y - EDITOR_PAD_TOP) / CODE_LINE)
            .floor()
            .max(0.0) as u32;
        let last_line = first_line + (self.viewport_height / CODE_LINE).ceil() as u32 + 1;
        doc.diagnostics
            .iter()
            .filter(|d| (first_line..=last_line).contains(&d.span.start.line))
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
}
