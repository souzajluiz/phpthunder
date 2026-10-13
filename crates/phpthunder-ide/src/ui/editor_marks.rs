//! Reading aids drawn around the editor text: the bracket pair at the
//! caret, other occurrences of the identifier at the caret, indentation
//! guides, visible whitespace, and sticky lines that keep the enclosing
//! class and function headers in view while scrolling.

use super::*;
use crate::app::{App, Message, col_left, row_top};
use crate::editor::{Document, WrapLayout};
use iced::widget::{button, container, pin, row, space, text};
use iced::{Color, Element, Fill, Padding, Theme};

/// Element type of the editor views.
type El<'a> = Element<'a, Message>;

/// Most sticky lines shown at once.
const MAX_STICKY: usize = 3;

/// A tinted box over `width` columns at `(line, col)`, drawn under the
/// text (find marks style).
fn text_box<'a>(
    layout: &WrapLayout,
    line: usize,
    col: usize,
    width: usize,
    fill: Color,
    stroke: Option<Color>,
) -> El<'a> {
    pin(container(space())
        .width(width.max(1) as f32 * CHAR_WIDTH)
        .height(CODE_LINE - 2.0)
        .style(move |_: &Theme| container::Style {
            background: Some(fill.into()),
            border: iced::Border {
                color: stroke.unwrap_or(Color::TRANSPARENT),
                width: if stroke.is_some() { 1.0 } else { 0.0 },
                radius: 3.0.into(),
            },
            ..Default::default()
        }))
    .x(col_left(layout, col))
    .y(row_top(layout, line, col) + 1.0)
    .into()
}

impl App {
    /// Under-text marks for the caret: both brackets of the pair touching
    /// it, and every visible occurrence of the identifier at it (when it
    /// occurs more than once).
    pub(crate) fn caret_marks<'a>(&'a self, doc: &'a Document) -> Vec<El<'a>> {
        let layout = self.wrap_layout(doc);
        let mut marks = Vec::new();
        if let Some((a, b)) = doc.bracket_pair_at_caret() {
            for at in [a, b] {
                let pos = doc.position_of(at);
                let col = doc.visual_column(pos);
                marks.push(text_box(
                    &layout,
                    pos.line,
                    col,
                    1,
                    Color {
                        a: 0.35,
                        ..SELECTED
                    },
                    Some(Color {
                        a: 0.8,
                        ..BORDER_STRONG
                    }),
                ));
            }
        }
        if doc.selection().is_none()
            && let Some((start, end)) = doc.identifier_at_caret()
        {
            let word = &doc.text[start..end];
            let (first, last) = self.visible_lines(doc, &layout);
            let from = doc.line_start(first);
            let to = doc.line_end(last.min(doc.line_count().saturating_sub(1)));
            let all = doc.occurrences_in(word, 0, doc.text.len());
            if all.len() > 1 {
                for at in all.into_iter().filter(|a| (from..=to).contains(a)) {
                    let pos = doc.position_of(at);
                    let col = doc.visual_column(pos);
                    let width = word.chars().count();
                    marks.push(text_box(
                        &layout,
                        pos.line,
                        col,
                        width,
                        Color {
                            a: 0.55,
                            ..SELECTED
                        },
                        None,
                    ));
                }
            }
        }
        marks
    }

    /// One-pixel guides at each indentation level of the visible lines;
    /// blank lines take the deeper indentation of their neighbours so the
    /// guides run through them.
    pub(crate) fn indent_guides<'a>(&'a self, doc: &'a Document) -> Vec<El<'a>> {
        let layout = self.wrap_layout(doc);
        let (first, last) = self.visible_lines(doc, &layout);
        let last = last.min(doc.line_count().saturating_sub(1));
        let unit = if doc.indent_unit == "\t" {
            1
        } else {
            doc.indent_unit.len().max(1)
        };
        let depth = |line: usize| -> Option<usize> {
            let text = doc.line_text(line);
            (!text.trim().is_empty()).then(|| {
                let ws = &text[..text.len() - text.trim_start().len()];
                ws.chars()
                    .map(|c| if c == '\t' { unit } else { 1 })
                    .sum::<usize>()
                    / unit
            })
        };
        let mut out = Vec::new();
        for line in first..=last {
            let level = depth(line).unwrap_or_else(|| {
                let up = (0..line).rev().find_map(depth).unwrap_or(0);
                let down = (line + 1..doc.line_count()).find_map(depth).unwrap_or(0);
                up.min(down)
            });
            for l in 1..level {
                let col = l * unit;
                out.push(
                    pin(container(space())
                        .width(1)
                        .height(CODE_LINE)
                        .style(fill(BORDER)))
                    .x(col_left(&layout, col))
                    .y(row_top(&layout, line, col))
                    .into(),
                );
            }
        }
        out
    }

    /// Dots for spaces and arrows for tabs on the visible lines, when
    /// whitespace display is on.
    pub(crate) fn whitespace_marks<'a>(&'a self, doc: &'a Document) -> Vec<El<'a>> {
        if !self.show_whitespace {
            return Vec::new();
        }
        let layout = self.wrap_layout(doc);
        let (first, last) = self.visible_lines(doc, &layout);
        let last = last.min(doc.line_count().saturating_sub(1));
        let mut out = Vec::new();
        for line in first..=last {
            for (col, c) in doc.line_text(line).chars().enumerate() {
                let glyph = match c {
                    ' ' => "·",
                    '\t' => "→",
                    _ => continue,
                };
                out.push(
                    pin(
                        container(text(glyph).font(MONO).size(CODE_SIZE).color(GUTTER))
                            .height(CODE_LINE)
                            .center_y(CODE_LINE),
                    )
                    .x(col_left(&layout, col))
                    .y(row_top(&layout, line, col))
                    .into(),
                );
            }
        }
        out
    }

    /// Headers of the blocks enclosing the first visible line, pinned to
    /// the top of the editor; clicking one jumps to it. `None` at the top
    /// of the file or outside any block.
    pub(crate) fn sticky_lines<'a>(&'a self, doc: &'a Document) -> Option<El<'a>> {
        let layout = self.wrap_layout(doc);
        let (first, _) = self.visible_lines(doc, &layout);
        // The pinned rows cover the lines under them, so the headers are
        // those of the first line below the pinned rows; settle on a count.
        let mut headers = Vec::new();
        for _ in 0..=MAX_STICKY {
            let covered = headers.len();
            let top = doc.scroll_y + covered as f32 * CODE_LINE;
            // The first line whose top is not hidden by the pinned rows.
            let row = ((top - EDITOR_PAD_TOP) / CODE_LINE).ceil().max(0.0) as usize;
            let line = layout
                .line_at_row(row)
                .0
                .max(first)
                .min(doc.line_count().saturating_sub(1));
            let mut next = doc.enclosing_headers(line, MAX_STICKY);
            // A header still in view below the pinned rows needs no copy.
            next.retain(|&h| row_top(&layout, h, 0) < top + EDITOR_PAD_TOP);
            if next.len() == covered {
                headers = next;
                break;
            }
            headers = next;
        }
        if headers.is_empty() {
            return None;
        }
        let rows = headers.into_iter().map(|line| {
            let number = container(
                text((line + 1).to_string())
                    .font(MONO)
                    .size(CODE_SIZE)
                    .color(GUTTER),
            )
            .width(GUTTER_WIDTH)
            .align_right(GUTTER_WIDTH)
            .padding(Padding {
                right: 16.0,
                ..Padding::ZERO
            });
            let code = text(doc.line_text(line).trim_end().to_string())
                .font(MONO)
                .size(CODE_SIZE)
                .color(SYN_VARIABLE)
                .wrapping(iced::widget::text::Wrapping::None);
            let line_button = button(
                row![
                    number,
                    container(code).padding(Padding {
                        left: EDITOR_PAD_LEFT,
                        ..Padding::ZERO
                    })
                ]
                .height(CODE_LINE)
                .align_y(iced::Center),
            )
            .padding(0)
            .width(Fill)
            .style(|_: &Theme, status| button::Style {
                background: interaction_wash(status),
                ..Default::default()
            })
            .on_press(Message::JumpToLine(line));
            // The wash is translucent: an opaque island underneath hides
            // the scrolled text.
            container(line_button).style(fill(BG)).into()
        });
        Some(
            iced::widget::column![
                iced::widget::Column::with_children(rows),
                container(space()).width(Fill).height(1).style(fill(BORDER)),
            ]
            .width(Fill)
            .into(),
        )
    }
}
