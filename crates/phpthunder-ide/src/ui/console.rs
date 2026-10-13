//! Drawing the integrated terminal from the VT100 screen.

use super::*;
use crate::app::{App, Bottom, Message, TERMINAL_SINK};
use crate::tr;
use iced::widget::text::{LineHeight, Span, Wrapping};
use iced::widget::{Id, button, column, container, mouse_area, rich_text, row, sensor, text_input};
use iced::{Center, Color, Element, Fill, Font, Theme, mouse};

/// Terminal font size.
const TERMINAL_SIZE: f32 = 13.0;
/// Height of one terminal row, in pixels.
pub const TERMINAL_LINE: f32 = 18.0;
/// Width of one terminal column (Geist Mono advances 0.6 em).
pub const TERMINAL_CHAR_WIDTH: f32 = TERMINAL_SIZE * 0.6;
/// Padding around the terminal grid.
pub const TERMINAL_PADDING: f32 = 8.0;

/// The 16 ANSI colors, tuned to the Thunder Purple palette.
const ANSI: [u32; 16] = [
    0x1E222B, 0xF7768E, 0x9ECE6A, 0xE0AF68, 0x7AA2F7, 0xBB9AF7, 0x5FD4C4, 0xC3C8D2, //
    0x4A5163, 0xFF8FA3, 0xB9F27C, 0xF2C14E, 0x9AB8FF, 0xD0B3FF, 0x7FE8DA, 0xFFFFFF,
];

/// Converts a terminal color; `None` is the default (theme) color.
fn ansi_color(color: vt100::Color) -> Option<Color> {
    match color {
        vt100::Color::Default => None,
        vt100::Color::Idx(i) if i < 16 => Some(rgb(ANSI[i as usize])),
        vt100::Color::Idx(i) if i < 232 => {
            // 6x6x6 color cube.
            let i = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Some(Color::from_rgb8(
                level(i / 36),
                level((i / 6) % 6),
                level(i % 6),
            ))
        }
        vt100::Color::Idx(i) => {
            let gray = 8 + (i - 232) * 10;
            Some(Color::from_rgb8(gray, gray, gray))
        }
        vt100::Color::Rgb(r, g, b) => Some(Color::from_rgb8(r, g, b)),
    }
}

/// One run of identically styled characters in a row.
struct Run {
    /// Characters of the run.
    text: String,
    /// Foreground color.
    fg: Color,
    /// Background color, if not the panel's.
    bg: Option<Color>,
}

/// Splits the visible screen into styled runs per row, drawing the cursor
/// as an inverted cell when `show_cursor`.
fn screen_runs(screen: &vt100::Screen, show_cursor: bool) -> Vec<Vec<Run>> {
    let (rows, cols) = screen.size();
    let (cursor_row, cursor_col) = screen.cursor_position();
    let cursor_visible = show_cursor && !screen.hide_cursor() && screen.scrollback() == 0;
    (0..rows)
        .map(|r| {
            let mut runs: Vec<Run> = Vec::new();
            for c in 0..cols {
                let Some(cell) = screen.cell(r, c) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }
                let mut fg = ansi_color(cell.fgcolor()).unwrap_or(TEXT);
                let mut bg = ansi_color(cell.bgcolor());
                // Bold uses the bright variant of the 8 basic colors.
                if cell.bold()
                    && let vt100::Color::Idx(i) = cell.fgcolor()
                    && i < 8
                {
                    fg = rgb(ANSI[i as usize + 8]);
                }
                if cell.dim() {
                    fg = Color { a: 0.6, ..fg };
                }
                if cell.inverse() || (cursor_visible && r == cursor_row && c == cursor_col) {
                    let back = bg.unwrap_or(BG);
                    bg = Some(fg);
                    fg = back;
                }
                let contents = cell.contents();
                let ch = if contents.is_empty() { " " } else { contents };
                match runs.last_mut() {
                    Some(run) if run.fg == fg && run.bg == bg => run.text.push_str(ch),
                    _ => runs.push(Run {
                        text: ch.to_string(),
                        fg,
                        bg,
                    }),
                }
            }
            runs
        })
        .collect()
}

impl App {
    /// Body of console panel `panel` (terminal or Claude): the screen grid,
    /// an exited banner, and the hidden input that holds focus while it is
    /// the active console.
    pub(crate) fn terminal_body(&self, panel: Bottom) -> Element<'_, Message> {
        let Some(session) = self.session_of(panel) else {
            return container(
                row![
                    label(tr!("terminal-none"), 12.0, MUTED),
                    button(label(tr!("terminal-start"), 12.0, TEXT_SOFT))
                        .padding([4, 10])
                        .style(ghost_button)
                        .on_press(Message::TerminalRestart(panel)),
                ]
                .spacing(12)
                .align_y(Center),
            )
            .padding([10, 16])
            .into();
        };
        let focused = self.terminal_focused && self.console == panel;
        let rows = session.with_screen(|screen| screen_runs(screen, focused));
        let lines = rows.into_iter().map(|runs| {
            let spans: Vec<Span<'_, (), Font>> = runs
                .into_iter()
                .map(|run| {
                    let span = Span::new(run.text).color(run.fg);
                    match run.bg {
                        Some(bg) => span.background(bg),
                        None => span,
                    }
                })
                .collect();
            rich_text(spans)
                .font(MONO)
                .size(TERMINAL_SIZE)
                .line_height(LineHeight::Absolute(TERMINAL_LINE.into()))
                .wrapping(Wrapping::None)
                .into()
        });
        let grid = column(lines).padding(TERMINAL_PADDING);
        let mut layers = column![grid].width(Fill).height(Fill);
        if session.has_exited() {
            layers = layers.push(
                row![
                    label(tr!("terminal-exited"), 12.0, MUTED),
                    button(label(tr!("terminal-restart"), 12.0, TEXT_SOFT))
                        .padding([4, 10])
                        .style(ghost_button)
                        .on_press(Message::TerminalRestart(panel)),
                ]
                .spacing(12)
                .align_y(Center)
                .padding([4, 16]),
            );
        }
        // CONTEXT: focusing this empty input moves keyboard focus away from
        // the code editor; its key events are routed to the shell.
        let sink = text_input("", "")
            .id(Id::new(TERMINAL_SINK))
            .size(1)
            .padding(0)
            .width(1)
            .style(|_: &Theme, _| text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: iced::Border::default(),
                icon: Color::TRANSPARENT,
                placeholder: Color::TRANSPARENT,
                value: Color::TRANSPARENT,
                selection: Color::TRANSPARENT,
            });
        let body = mouse_area(
            container(column![layers, container(sink).height(1)])
                .width(Fill)
                .height(Fill)
                .style(fill(BG)),
        )
        .on_press(Message::TerminalFocus(panel))
        .on_scroll(move |delta| {
            let lines = match delta {
                mouse::ScrollDelta::Lines { y, .. } => (y * 3.0) as i32,
                mouse::ScrollDelta::Pixels { y, .. } => (y / TERMINAL_LINE) as i32,
            };
            Message::TerminalScroll(panel, lines)
        })
        .interaction(mouse::Interaction::Text);
        sensor(body)
            .on_show(move |size| Message::TerminalResized(panel, size))
            .on_resize(move |size| Message::TerminalResized(panel, size))
            .into()
    }
}
