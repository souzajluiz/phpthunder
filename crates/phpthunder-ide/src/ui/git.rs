//! Git views: the Commit panel (changes, message, actions, diff) and the
//! change marks drawn in the editor gutter.

use super::*;
use crate::app::{App, Message};
use crate::editor::Document;
use crate::git::{Change, DiffKind, LineMark};
use crate::tr;
use iced::widget::{button, column, container, pin, row, scrollable, space, text_input, tooltip};
use iced::{Center, Color, Element, Fill, Padding, Theme};

/// Width of the change list column of the Commit panel.
const CHANGES_WIDTH: f32 = 380.0;

/// Width of a gutter change mark.
const MARK_WIDTH: f32 = 3.0;

/// Color of added lines and files.
const ADDED: Color = rgb(0x9ECE6A);

/// Color of modified lines and files.
const MODIFIED: Color = rgb(0x7AA2F7);

/// Color of deleted lines and files.
const DELETED: Color = rgb(0xF7768E);

/// Color of untracked files.
const UNTRACKED: Color = rgb(0xD19A66);

/// File-name color for a change kind; used by the project tree and the
/// Commit panel.
pub(crate) fn change_color(change: Change) -> Color {
    match change {
        Change::Modified | Change::Renamed => MODIFIED,
        Change::Added => ADDED,
        Change::Deleted | Change::Conflicted => DELETED,
        Change::Untracked => UNTRACKED,
    }
}

/// A small icon button with a tooltip.
fn icon_button<'a>(which: Icon, tip: String, msg: Option<Message>) -> Element<'a, Message> {
    tooltip(
        button(icon(
            which,
            14.0,
            2.0,
            if msg.is_some() { TEXT_SOFT } else { GUTTER },
        ))
        .padding(5)
        .style(ghost_button)
        .on_press_maybe(msg),
        container(label(tip, 12.0, TEXT))
            .padding([4, 8])
            .style(popup),
        tooltip::Position::Top,
    )
    .into()
}

impl App {
    /// The Commit panel: changed files with staging checkboxes, the commit
    /// message and actions on the left, the selected file's diff on the right.
    pub(crate) fn commit_body(&self) -> Element<'_, Message> {
        let Some(status) = &self.git else {
            return container(label(tr!("git-not-repository"), 12.0, MUTED))
                .padding([10, 16])
                .into();
        };
        let busy = self.commit.busy.is_some();
        let idle = |msg: Message| (!busy).then_some(msg);
        let mut sync = String::new();
        if status.ahead > 0 {
            sync.push_str(&format!("↑{} ", status.ahead));
        }
        if status.behind > 0 {
            sync.push_str(&format!("↓{}", status.behind));
        }
        let header = row![
            button(
                row![
                    icon(Icon::Branch, 13.0, 2.0, MUTED),
                    label(
                        status.branch.clone().unwrap_or_else(|| tr!("git-detached")),
                        12.0,
                        TEXT
                    )
                ]
                .spacing(6)
                .align_y(Center)
            )
            .padding([2, 6])
            .style(ghost_button)
            .on_press(Message::GitShowBranches),
            label(sync, 12.0, ACCENT),
            space().width(Fill),
            icon_button(Icon::Refresh, tr!("git-refresh"), idle(Message::GitRefresh)),
            icon_button(Icon::Pull, tr!("git-pull"), idle(Message::GitPull)),
            icon_button(Icon::Push, tr!("git-push"), idle(Message::GitPush)),
        ]
        .spacing(4)
        .align_y(Center);

        let all_staged =
            !status.files.is_empty() && status.files.iter().all(|f| f.staged && !f.unstaged);
        let rows = status.files.iter().map(|f| {
            let staged = f.staged && !f.unstaged;
            let partly = f.staged && f.unstaged;
            let check = button(
                container(if staged || partly {
                    icon(Icon::Check, 11.0, 3.0, if partly { MUTED } else { ACCENT }).into()
                } else {
                    Element::from(space())
                })
                .center(14),
            )
            .padding(0)
            .width(16)
            .height(16)
            .style(toggle_button(staged))
            .on_press_maybe(idle(Message::GitToggleStage(f.path.clone())));
            let selected = self.commit.selected.as_ref() == Some(&f.path);
            let discard: Element<'_, Message> = if f.change == Change::Untracked {
                space().width(24).into()
            } else if self.commit.confirm_discard.as_ref() == Some(&f.path) {
                button(label(tr!("git-discard-confirm"), 11.0, DELETED))
                    .padding([2, 6])
                    .style(ghost_button)
                    .on_press_maybe(idle(Message::GitDiscard(f.path.clone())))
                    .into()
            } else {
                icon_button(
                    Icon::Undo,
                    tr!("git-discard"),
                    idle(Message::GitDiscard(f.path.clone())),
                )
            };
            let dir = std::path::Path::new(&f.rel)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = f
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            row![
                check,
                button(
                    row![
                        label(f.change.letter(), 12.0, change_color(f.change))
                            .font(MONO)
                            .width(12),
                        label(name, 13.0, change_color(f.change)),
                        label(dir, 11.0, MUTED),
                    ]
                    .spacing(8)
                    .align_y(Center)
                )
                .on_press(Message::GitSelect(f.path.clone()))
                .width(Fill)
                .height(24)
                .padding([0, 6])
                .style(row_button(selected)),
                discard,
            ]
            .spacing(6)
            .align_y(Center)
            .into()
        });
        let list: Element<'_, Message> = if status.files.is_empty() {
            container(label(tr!("git-clean"), 12.0, MUTED))
                .padding([8, 4])
                .into()
        } else {
            scrollable(column(rows).spacing(1).padding(Padding {
                right: 10.0,
                ..Padding::ZERO
            }))
            .height(Fill)
            .style(scrollbars)
            .into()
        };
        let stage_all = button(label(
            if all_staged {
                tr!("git-unstage-all")
            } else {
                tr!("git-stage-all")
            },
            11.0,
            TEXT_SOFT,
        ))
        .padding([2, 6])
        .style(ghost_button)
        .on_press_maybe(idle(Message::GitStageAll(!all_staged)));
        let message = text_input(&tr!("git-message-placeholder"), &self.commit.message)
            .on_input(Message::GitCommitText)
            .on_submit(Message::GitCommit(false))
            .size(13)
            .padding([6, 8])
            .style(boxed_input);
        let can_commit = !busy && status.has_staged() && !self.commit.message.trim().is_empty();
        let actions: Element<'_, Message> = match &self.commit.busy {
            Some(op) => label(op.progress(), 12.0, MUTED).into(),
            None => row![
                button(label(tr!("git-commit"), 12.0, rgb(0x14161C)).font(UI_MEDIUM))
                    .padding([5, 12])
                    .style(primary_button)
                    .on_press_maybe(can_commit.then_some(Message::GitCommit(false))),
                button(label(tr!("git-commit-push"), 12.0, TEXT_SOFT))
                    .padding([5, 10])
                    .style(ghost_button)
                    .on_press_maybe(can_commit.then_some(Message::GitCommit(true))),
            ]
            .spacing(6)
            .align_y(Center)
            .into(),
        };
        let left = column![
            header,
            list,
            row![stage_all, space().width(Fill)],
            message,
            actions
        ]
        .spacing(6)
        .padding([8, 12])
        .width(CHANGES_WIDTH);

        row![left, vsep(), self.diff_view()].into()
    }

    /// The diff of the selected file: hunk headers, removed and added lines
    /// on tinted rows; clicking a line opens the file there.
    fn diff_view(&self) -> Element<'_, Message> {
        if self.commit.selected.is_none() {
            return container(label(tr!("git-select-file"), 12.0, MUTED))
                .padding([10, 16])
                .into();
        }
        let rows = self.commit.diff.iter().take(2000).map(|line| {
            let (color, background, sign) = match line.kind {
                DiffKind::Hunk => (MUTED, Color::TRANSPARENT, ""),
                DiffKind::Context => (TEXT_SOFT, Color::TRANSPARENT, " "),
                DiffKind::Added => (TEXT, Color { a: 0.12, ..ADDED }, "+"),
                DiffKind::Removed => (TEXT, Color { a: 0.12, ..DELETED }, "-"),
            };
            let number = line.new_line.map(|n| n.to_string()).unwrap_or_default();
            button(
                row![
                    label(number, 11.0, GUTTER).font(MONO).width(40),
                    label(format!("{sign}{}", line.text), 12.0, color).font(MONO),
                ]
                .spacing(8),
            )
            .padding([0, 8])
            .width(Fill)
            .style(move |_: &Theme, _| button::Style {
                background: Some(background.into()),
                text_color: color,
                ..Default::default()
            })
            .on_press_maybe(line.new_line.map(Message::GitOpenLine))
            .into()
        });
        scrollable(column(rows).padding([4, 0]))
            .height(Fill)
            .width(Fill)
            .style(scrollbars)
            .into()
    }

    /// Change marks beside the line numbers: bars for added and modified
    /// lines, a short red tick where lines were deleted.
    pub(crate) fn git_gutter_marks<'a>(&'a self, doc: &'a Document) -> Vec<Element<'a, Message>> {
        let Some(marks) = self.git_marks.get(&doc.path) else {
            return Vec::new();
        };
        let x = GUTTER_WIDTH - 8.0;
        marks
            .iter()
            .map(|m| {
                let (color, height, y, width) = match m.mark {
                    LineMark::Added => (
                        ADDED,
                        m.count as f32 * CODE_LINE,
                        m.line as f32 * CODE_LINE,
                        MARK_WIDTH,
                    ),
                    LineMark::Modified => (
                        MODIFIED,
                        m.count as f32 * CODE_LINE,
                        m.line as f32 * CODE_LINE,
                        MARK_WIDTH,
                    ),
                    LineMark::Deleted => (DELETED, 2.0, m.line as f32 * CODE_LINE - 1.0, 8.0),
                };
                pin(container(space())
                    .width(width)
                    .height(height)
                    .style(fill(color)))
                .x(x)
                .y(EDITOR_PAD_TOP + y.max(0.0))
                .into()
            })
            .collect()
    }
}
