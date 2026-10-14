//! Reading aids drawn around the editor text: the bracket pair at the
//! caret, other occurrences of the identifier at the caret, indentation
//! guides, visible whitespace, and sticky lines that keep the enclosing
//! class and function headers in view while scrolling.

use super::*;
use crate::app::{App, Message, col_left, row_top};
use crate::editor::{Document, WrapLayout};
use crate::tr;
use iced::widget::{button, container, pin, row, space, text};
use iced::{Color, Element, Fill, Padding, Theme};

/// Element type of the editor views.
type El<'a> = Element<'a, Message>;

/// A small clickable icon for the gutter with a tooltip.
fn gutter_button<'a>(glyph: Icon, tip: String, message: Message) -> El<'a> {
    iced::widget::tooltip(
        button(icon(glyph, 12.0, 2.0, MUTED))
            .padding(1)
            .style(ghost_button)
            .on_press(message),
        container(label(tip, 12.0, TEXT))
            .padding([4, 8])
            .style(popup),
        iced::widget::tooltip::Position::Right,
    )
    .into()
}

/// The hex colors written in `line` (`#rgb`, `#rgba`, `#rrggbb`,
/// `#rrggbbaa`), at most four.
fn hex_colors(line: &str) -> Vec<Color> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() && out.len() < 4 {
        if bytes[i] != b'#' || (i > 0 && bytes[i - 1].is_ascii_alphanumeric()) {
            i += 1;
            continue;
        }
        let digits = bytes[i + 1..]
            .iter()
            .take_while(|b| b.is_ascii_hexdigit())
            .count();
        let after = bytes.get(i + 1 + digits);
        if matches!(digits, 3 | 4 | 6 | 8) && after.is_none_or(|b| !b.is_ascii_alphanumeric()) {
            let hex = &line[i + 1..i + 1 + digits];
            let v = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
            let short = |c: char| v(&format!("{c}{c}"));
            let chars: Vec<char> = hex.chars().collect();
            let (r, g, b, a) = match digits {
                3 | 4 => (
                    short(chars[0]),
                    short(chars[1]),
                    short(chars[2]),
                    chars.get(3).map_or(255, |c| short(*c)),
                ),
                _ => (
                    v(&hex[0..2]),
                    v(&hex[2..4]),
                    v(&hex[4..6]),
                    hex.get(6..8).map_or(255, v),
                ),
            };
            out.push(Color::from_rgba8(r, g, b, f32::from(a) / 255.0));
        }
        i += 1 + digits;
    }
    out
}

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

    /// Gutter arrows beside methods and classes on visible lines: up when
    /// the method overrides or implements one in a supertype (click jumps
    /// there), down when subclasses override it or extend the class (click
    /// lists them).
    pub(crate) fn hierarchy_marks<'a>(&'a self, doc: &'a Document) -> Vec<El<'a>> {
        use phpthunder::symbol::{Kind, MemberTag};
        if !doc.is_php() {
            return Vec::new();
        }
        let layout = self.wrap_layout(doc);
        let (first, last) = self.visible_lines(doc, &layout);
        let mut out = Vec::new();
        for s in self.lang.file_symbols(&doc.path) {
            let line = s.name_span.start.line as usize;
            if !(first..=last).contains(&line) || s.is_virtual {
                continue;
            }
            let (up, down) = match s.kind {
                Kind::Method => (
                    self.lang.super_member(&s.fqn, MemberTag::Method, &s.name),
                    !self.lang.overriding_methods(&s.fqn, &s.name).is_empty(),
                ),
                k if k.is_class_like() => (None, !self.lang.subtypes(&s.fqn).is_empty()),
                _ => continue,
            };
            let y = row_top(&layout, line, 0) + (CODE_LINE - 14.0) / 2.0;
            let mut marks: Vec<El<'a>> = Vec::new();
            if let Some((path, target)) = up {
                let owner = target
                    .fqn
                    .rsplit('\\')
                    .next()
                    .unwrap_or(&target.fqn)
                    .to_string();
                marks.push(gutter_button(
                    Icon::StepOut,
                    tr!(
                        "gutter-overrides",
                        target = format!("{owner}::{}()", target.name)
                    ),
                    Message::OpenAt(path, target.name_span.start),
                ));
            }
            if down {
                marks.push(gutter_button(
                    Icon::StepInto,
                    tr!("gutter-overridden"),
                    Message::ImplementationsAt(s.name_start),
                ));
            }
            if !marks.is_empty() {
                out.push(
                    pin(row(marks).spacing(0))
                        .x(GUTTER_WIDTH - 15.0)
                        .y(y)
                        .into(),
                );
            }
        }
        out
    }

    /// A swatch past the end of each visible line for every hex color
    /// (`#rgb`, `#rrggbb`, `#rrggbbaa`) written on it.
    pub(crate) fn color_swatches<'a>(&'a self, doc: &'a Document) -> Vec<El<'a>> {
        let layout = self.wrap_layout(doc);
        let (first, last) = self.visible_lines(doc, &layout);
        let last = last.min(doc.line_count().saturating_sub(1));
        let mut out = Vec::new();
        for line in first..=last {
            let text = doc.line_text(line);
            let colors = hex_colors(text);
            if colors.is_empty() {
                continue;
            }
            let end = doc.visual_column(doc.position_of(doc.line_end(line))) + 2;
            for (i, color) in colors.into_iter().enumerate() {
                let col = end + i * 2;
                out.push(
                    pin(container(space())
                        .width(11)
                        .height(11)
                        .style(move |_: &Theme| container::Style {
                            background: Some(color.into()),
                            border: iced::Border {
                                color: BORDER_STRONG,
                                width: 1.0,
                                radius: 2.0.into(),
                            },
                            ..Default::default()
                        }))
                    .x(col_left(&layout, col))
                    .y(row_top(&layout, line, col) + (CODE_LINE - 11.0) / 2.0)
                    .into(),
                );
            }
        }
        out
    }

    /// Git Annotate: author and date of the last change, past the end of
    /// each visible line, while Annotate is on for the file.
    pub(crate) fn blame_hints<'a>(&'a self, doc: &'a Document) -> Vec<El<'a>> {
        let Some(lines) = self.blame.get(&doc.path) else {
            return Vec::new();
        };
        let layout = self.wrap_layout(doc);
        let (first, last) = self.visible_lines(doc, &layout);
        (first..=last.min(doc.line_count().saturating_sub(1)))
            .filter_map(|line| {
                let b = lines.get(line)?;
                let text = if b.author.is_empty() {
                    tr!("annotate-uncommitted")
                } else {
                    format!("{} · {} · {}", b.author, b.date, b.summary)
                };
                let col = doc.visual_column(doc.position_of(doc.line_end(line))) + 4;
                Some(
                    pin(container(label(text, 12.0, GUTTER))
                        .height(CODE_LINE)
                        .center_y(CODE_LINE))
                    .x(col_left(&layout, col))
                    .y(row_top(&layout, line, col))
                    .into(),
                )
            })
            .collect()
    }

    /// Coverage bars at the left edge of the gutter: green beside
    /// statements the last run with coverage executed, red beside the ones
    /// it missed.
    pub(crate) fn coverage_marks<'a>(&'a self, doc: &'a Document) -> Vec<El<'a>> {
        let Some(file) = self.coverage.get(&doc.path) else {
            return Vec::new();
        };
        let layout = self.wrap_layout(doc);
        let bar = |line: u32, color: iced::Color| -> Option<El<'a>> {
            let line = (line as usize)
                .checked_sub(1)
                .filter(|l| *l < doc.line_count())?;
            Some(
                pin(container(space())
                    .width(3)
                    .height(CODE_LINE)
                    .style(move |_: &iced::Theme| container::Style {
                        background: Some(color.into()),
                        ..Default::default()
                    }))
                .x(0.0)
                .y(row_top(&layout, line, 0))
                .into(),
            )
        };
        file.covered
            .iter()
            .filter_map(|&l| bar(l, rgb(0x9ECE6A)))
            .chain(file.uncovered.iter().filter_map(|&l| bar(l, rgb(0xF7768E))))
            .collect()
    }

    /// A bookmark glyph in the gutter beside each bookmarked line.
    pub(crate) fn bookmark_marks<'a>(&'a self, doc: &'a Document) -> Vec<El<'a>> {
        let Some(lines) = self.bookmarks.get(&doc.path) else {
            return Vec::new();
        };
        let layout = self.wrap_layout(doc);
        lines
            .iter()
            .filter(|l| **l < doc.line_count())
            .map(|&line| {
                pin(container(icon(Icon::Bookmark, 12.0, 2.0, ACCENT))
                    .height(CODE_LINE)
                    .center_y(CODE_LINE))
                .x(4.0)
                .y(row_top(&layout, line, 0))
                .into()
            })
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_should_read_short_and_long_forms() {
        assert_eq!(hex_colors("color: #fff; bg: #1D1D26;").len(), 2);
    }

    #[test]
    fn hex_colors_should_skip_ids_and_odd_lengths() {
        assert!(hex_colors("a#fff #12345 #abcdefg").is_empty());
    }
}
