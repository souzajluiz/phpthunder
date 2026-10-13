//! The debugger in the IDE: debugging the Pest (or PHPUnit) test at the
//! caret under Xdebug, breakpoints from the gutter, stepping, and the
//! Debug panel's frames, variables, output and evaluation.

use super::*;
use crate::debug::{self, DebugCommand, DebugEvent, DebugLaunch};
use crate::testing::{detect_framework, scope_arguments};
use crate::tr;
use iced::Task;
use phpthunder::syntax::Pos;
use std::path::PathBuf;

/// Most output lines kept in the Debug panel.
const MAX_OUTPUT: usize = 2000;

impl App {
    /// Starts debugging the test at the caret (else its file, else the
    /// suite) with the project's Pest or PHPUnit, stopping a running
    /// session first. Opens the Debug panel.
    pub(crate) fn debug_test(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let Some((_, bin)) = detect_framework(&root) else {
            self.notice = Some(tr!("error-no-test-runner"));
            return Task::none();
        };
        if let Some(previous) = &self.debug {
            let _ = previous.commands.send(DebugCommand::Stop);
        }
        self.save_all_php();
        let scope = self.test_scope(TestTarget::Context);
        let mut program = vec![bin.into_os_string(), "--colors=never".into()];
        program.extend(scope_arguments(&scope));
        let breakpoints = self
            .breakpoints
            .iter()
            .flat_map(|(file, lines)| lines.iter().map(move |l| (file.clone(), *l)))
            .collect();
        let (commands, events) = debug::start_session(DebugLaunch {
            root,
            program,
            breakpoints,
        });
        self.debug = Some(DebugView {
            commands,
            title: scope.label(),
            state: DebugState::Starting,
            frames: Vec::new(),
            frame: 0,
            variables: Vec::new(),
            children: Default::default(),
            expanded: Default::default(),
            output: Vec::new(),
            eval: String::new(),
            evaluations: Vec::new(),
        });
        self.bottom = Some(Bottom::Debug);
        Task::run(events, Message::Debug)
    }

    /// Sends a command to the running session (no-op without one).
    pub(crate) fn debug_send(&mut self, command: DebugCommand) -> Task<Message> {
        if let Some(d) = self
            .debug
            .as_ref()
            .filter(|d| d.state != DebugState::Finished)
        {
            let _ = d.commands.send(command);
        }
        Task::none()
    }

    /// Applies a session event to the panel; on a pause, opens the file at
    /// the execution point.
    pub(crate) fn debug_event(&mut self, event: DebugEvent) -> Task<Message> {
        let Some(view) = &mut self.debug else {
            return Task::none();
        };
        match event {
            DebugEvent::Output(line) => {
                view.output.push(line);
                if view.output.len() > MAX_OUTPUT {
                    view.output.drain(..view.output.len() - MAX_OUTPUT);
                }
            }
            DebugEvent::Connected | DebugEvent::Running => view.state = DebugState::Running,
            DebugEvent::Paused(frames) => {
                view.state = DebugState::Paused;
                view.frames = frames;
                view.frame = 0;
                view.variables.clear();
                view.children.clear();
                view.expanded.clear();
                self.bottom = Some(Bottom::Debug);
                return self.show_execution_point();
            }
            DebugEvent::Variables(depth, variables) => {
                if depth == view.frame {
                    view.variables = variables;
                }
            }
            DebugEvent::Children(fullname, members) => {
                view.children.insert(fullname, members);
            }
            DebugEvent::Evaluated(expression, result) => {
                view.evaluations.push((expression, result))
            }
            DebugEvent::Finished(error) => {
                view.state = DebugState::Finished;
                view.frames.clear();
                self.notice = Some(match error {
                    Some(error) => tr!("debug-failed", error = error),
                    None => tr!("debug-finished"),
                });
            }
        }
        Task::none()
    }

    /// Opens the selected frame's file at its line.
    fn show_execution_point(&mut self) -> Task<Message> {
        let Some((path, line)) = self
            .debug
            .as_ref()
            .and_then(|d| d.execution_point())
            .map(|(p, l)| (p.to_path_buf(), l))
        else {
            return Task::none();
        };
        let line = u32::try_from(line).unwrap_or(0);
        self.open(path, Some(Pos { line, col: 0 }))
    }

    /// Selects a stack frame: loads its variables and shows its line.
    pub(crate) fn debug_select_frame(&mut self, level: usize) -> Task<Message> {
        let Some(view) = self
            .debug
            .as_mut()
            .filter(|d| d.state == DebugState::Paused)
        else {
            return Task::none();
        };
        view.frame = level;
        view.variables.clear();
        view.children.clear();
        view.expanded.clear();
        let _ = view.commands.send(DebugCommand::SelectFrame(level));
        self.show_execution_point()
    }

    /// Expands or collapses a variable, loading its members when Xdebug did
    /// not send them along.
    pub(crate) fn debug_toggle_variable(&mut self, fullname: String) -> Task<Message> {
        let Some(view) = self.debug.as_mut() else {
            return Task::none();
        };
        if !view.expanded.remove(&fullname) {
            view.expanded.insert(fullname.clone());
            if !view.children.contains_key(&fullname) && view.state == DebugState::Paused {
                let depth = view.frame;
                let _ = view.commands.send(DebugCommand::Expand { fullname, depth });
            }
        }
        Task::none()
    }

    /// Evaluates the typed expression in the paused script.
    pub(crate) fn debug_evaluate(&mut self) -> Task<Message> {
        let Some(view) = self
            .debug
            .as_mut()
            .filter(|d| d.state == DebugState::Paused)
        else {
            return Task::none();
        };
        let expression = std::mem::take(&mut view.eval);
        if !expression.trim().is_empty() {
            let _ = view.commands.send(DebugCommand::Eval(expression));
        }
        Task::none()
    }

    /// Adds or removes the breakpoint on 0-based `line` of `path`, telling a
    /// running session too.
    fn toggle_breakpoint(&mut self, path: PathBuf, line: usize) {
        let lines = self.breakpoints.entry(path.clone()).or_default();
        let command = if lines.remove(&line) {
            DebugCommand::RemoveBreakpoint(path, line)
        } else {
            lines.insert(line);
            DebugCommand::AddBreakpoint(path, line)
        };
        let _ = self.debug_send(command);
    }

    /// Toggles a breakpoint on the caret line (`⌘F8`).
    pub(crate) fn toggle_breakpoint_at_caret(&mut self) -> Task<Message> {
        if let Some((path, line)) = self
            .doc()
            .filter(|d| d.is_php())
            .map(|d| (d.path.clone(), d.cursor().line))
        {
            self.toggle_breakpoint(path, line);
        }
        Task::none()
    }

    /// Toggles a breakpoint on the gutter line under the mouse.
    pub(crate) fn toggle_breakpoint_at_gutter(&mut self) -> Task<Message> {
        let Some(doc) = self.doc().filter(|d| d.is_php()) else {
            return Task::none();
        };
        let row = ((self.gutter_y - crate::ui::EDITOR_PAD_TOP) / crate::ui::CODE_LINE).floor();
        if row < 0.0 {
            return Task::none();
        }
        let (line, _) = self.wrap_layout(doc).line_at_row(row as usize);
        if line >= doc.line_count() {
            return Task::none();
        }
        let path = doc.path.clone();
        self.toggle_breakpoint(path, line);
        Task::none()
    }
}
