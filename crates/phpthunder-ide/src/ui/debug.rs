//! Debugger views: the Debug panel (controls, call stack, variables,
//! output and evaluation) and breakpoint / execution marks in the editor.

use super::*;
use crate::app::{App, DebugState, DebugView, Message};
use crate::debug::{DebugCommand, Variable};
use crate::editor::Document;
use crate::keymap::{Cmd, hint};
use crate::tr;
use iced::widget::{button, column, container, pin, row, scrollable, space, text_input, tooltip};
use iced::{Center, Color, Element, Fill, Theme};

/// Breakpoint dot color.
const BREAKPOINT: Color = rgb(0xF7768E);

/// Execution line tint and arrow color.
const EXECUTION: Color = rgb(0xE0AF68);

/// Width of the call stack column.
const FRAMES_WIDTH: f32 = 300.0;

/// Width of the output and evaluation column.
const CONSOLE_WIDTH: f32 = 420.0;

/// Deepest variable nesting drawn.
const MAX_VARIABLE_DEPTH: usize = 8;

/// A debugger control button, enabled when `enabled`.
fn control<'a>(
    which: Icon,
    title: String,
    cmd: Cmd,
    command: DebugCommand,
    enabled: bool,
) -> Element<'a, Message> {
    tooltip(
        button(icon(
            which,
            15.0,
            2.0,
            if enabled { TEXT_SOFT } else { GUTTER },
        ))
        .padding(5)
        .style(ghost_button)
        .on_press_maybe(enabled.then_some(Message::DebugSend(command))),
        container(label(format!("{title}  {}", hint(cmd)), 12.0, TEXT))
            .padding([4, 8])
            .style(popup),
        tooltip::Position::Top,
    )
    .into()
}

/// Appends rows for `vars` (and expanded members) to `out`.
fn variable_rows<'a>(
    view: &'a DebugView,
    vars: &'a [Variable],
    depth: usize,
    out: &mut Vec<Element<'a, Message>>,
) {
    for var in vars {
        let expanded = view.expanded.contains(&var.fullname);
        let marker: Element<'a, Message> = if var.has_children {
            icon(
                if expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                11.0,
                2.2,
                MUTED,
            )
            .into()
        } else {
            space().width(11).into()
        };
        out.push(
            button(
                row![
                    space().width(depth as f32 * 16.0),
                    marker,
                    label(var.name.clone(), 12.0, SYN_VARIABLE).font(MONO),
                    label(var.kind.clone(), 11.0, MUTED),
                    label(var.value.clone(), 12.0, SYN_STRING).font(MONO),
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding([0, 8])
            .height(22)
            .width(Fill)
            .style(row_button(false))
            .on_press_maybe(
                var.has_children
                    .then(|| Message::DebugToggleVar(var.fullname.clone())),
            )
            .into(),
        );
        if expanded && depth < MAX_VARIABLE_DEPTH {
            let members = if var.children.is_empty() {
                view.children
                    .get(&var.fullname)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
            } else {
                var.children.as_slice()
            };
            variable_rows(view, members, depth + 1, out);
        }
    }
}

impl App {
    /// The Debug panel: controls and state, then the call stack, the
    /// selected frame's variables, and the process output with an
    /// evaluation field.
    pub(crate) fn debug_body(&self) -> Element<'_, Message> {
        let Some(view) = &self.debug else {
            return container(label(tr!("debug-hint"), 12.0, MUTED))
                .padding([10, 16])
                .into();
        };
        let paused = view.state == DebugState::Paused;
        let live = view.state != DebugState::Finished;
        let state = match view.state {
            DebugState::Starting => tr!("debug-starting"),
            DebugState::Running => tr!("debug-running"),
            DebugState::Paused => tr!("debug-paused"),
            DebugState::Finished => tr!("debug-ended"),
        };
        let controls = row![
            control(
                Icon::Resume,
                tr!("debug-resume"),
                Cmd::Resume,
                DebugCommand::Resume,
                paused
            ),
            control(
                Icon::StepOver,
                tr!("debug-step-over"),
                Cmd::StepOver,
                DebugCommand::StepOver,
                paused
            ),
            control(
                Icon::StepInto,
                tr!("debug-step-into"),
                Cmd::StepInto,
                DebugCommand::StepInto,
                paused
            ),
            control(
                Icon::StepOut,
                tr!("debug-step-out"),
                Cmd::StepOut,
                DebugCommand::StepOut,
                paused
            ),
            control(
                Icon::Stop,
                tr!("debug-stop"),
                Cmd::StopDebug,
                DebugCommand::Stop,
                live
            ),
            tooltip(
                button(icon(Icon::Bug, 15.0, 2.0, ACCENT))
                    .padding(5)
                    .style(ghost_button)
                    .on_press(Message::DebugTest),
                container(label(
                    format!("{}  {}", tr!("debug-again"), hint(Cmd::DebugTest)),
                    12.0,
                    TEXT
                ))
                .padding([4, 8])
                .style(popup),
                tooltip::Position::Top,
            ),
            label(view.title.clone(), 12.0, TEXT),
            label(state, 12.0, if paused { EXECUTION } else { MUTED }),
        ]
        .spacing(4)
        .align_y(Center)
        .padding([4, 8]);

        let frames = view.frames.iter().map(|f| {
            let file = f
                .file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            button(
                column![
                    label(f.function.clone(), 12.0, TEXT).font(MONO),
                    label(format!("{file}:{}", f.line + 1), 11.0, MUTED),
                ]
                .spacing(1),
            )
            .padding([2, 8])
            .width(Fill)
            .style(row_button(f.level == view.frame))
            .on_press(Message::DebugFrame(f.level))
            .into()
        });
        let frames = scrollable(column(frames).spacing(1).padding([4, 4]))
            .height(Fill)
            .style(scrollbars);

        let mut rows = Vec::new();
        variable_rows(view, &view.variables, 0, &mut rows);
        let variables = scrollable(column(rows).spacing(1).padding([4, 4]))
            .height(Fill)
            .width(Fill)
            .style(scrollbars);

        let mut console = column![].spacing(1).padding([4, 8]);
        for line in view.output.iter().rev().take(400).rev() {
            console = console.push(label(line.clone(), 12.0, TEXT_SOFT).font(MONO));
        }
        for (expression, result) in &view.evaluations {
            let (text, color) = match result {
                Ok(v) => (format!("{expression} = {} {}", v.kind, v.value), SYN_STRING),
                Err(e) => (format!("{expression}: {e}"), BREAKPOINT),
            };
            console = console.push(label(text, 12.0, color).font(MONO));
        }
        let eval = text_input(&tr!("debug-eval-placeholder"), &view.eval)
            .on_input(Message::DebugEvalText)
            .on_submit(Message::DebugEvalSubmit)
            .size(12)
            .font(MONO)
            .padding([4, 8])
            .style(boxed_input);
        let console = column![
            scrollable(console)
                .height(Fill)
                .width(Fill)
                .anchor_bottom()
                .style(scrollbars),
            container(eval).padding(4),
        ]
        .width(CONSOLE_WIDTH);

        column![
            controls,
            hsep(),
            row![
                container(frames).width(FRAMES_WIDTH),
                vsep(),
                variables,
                vsep(),
                console
            ]
            .height(Fill),
        ]
        .into()
    }

    /// Breakpoint dots in the gutter, plus the execution line band and
    /// arrow while paused in this file.
    pub(crate) fn debug_marks<'a>(&'a self, doc: &'a Document) -> Vec<Element<'a, Message>> {
        let mut marks: Vec<Element<'a, Message>> =
            self.breakpoints
                .get(&doc.path)
                .into_iter()
                .flatten()
                .map(|line| {
                    pin(container(space()).width(10).height(10).style(|_: &Theme| {
                        container::Style {
                            background: Some(BREAKPOINT.into()),
                            border: iced::border::rounded(5),
                            ..Default::default()
                        }
                    }))
                    .x(6.0)
                    .y(EDITOR_PAD_TOP + *line as f32 * CODE_LINE + (CODE_LINE - 10.0) / 2.0)
                    .into()
                })
                .collect();
        if let Some((path, line)) = self.debug.as_ref().and_then(|d| d.execution_point())
            && path == doc.path
        {
            let y = EDITOR_PAD_TOP + line as f32 * CODE_LINE;
            marks.push(
                pin(container(space())
                    .width(Fill)
                    .height(CODE_LINE)
                    .style(fill(Color {
                        a: 0.16,
                        ..EXECUTION
                    })))
                .y(y)
                .width(Fill)
                .height(Fill)
                .into(),
            );
            marks.push(
                pin(container(label("▶", 11.0, EXECUTION))
                    .height(CODE_LINE)
                    .center_y(CODE_LINE))
                .x(20.0)
                .y(y)
                .into(),
            );
        }
        marks
    }
}
