//! The Thunder Purple layout: title bar, tool rail, project tree, tabs,
//! breadcrumb, editor with completion/hover popups, usages panel, status bar.

use crate::app::{
    App, Bottom, Completion, EDITOR_ID, EDITOR_SCROLL_ID, Hover, Indexing, Message, Search,
    TestTarget, Usages, bindings,
};
use crate::editor::Document;
use crate::keymap::{Cmd, hint};
use crate::search::{self, Toggle};
use crate::tr;
use crate::ui::*;
use crate::ui::{Icon, Logo, icon, logo};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::text::{LineHeight, Span, Wrapping};
use iced::widget::{
    Stack, button, column, container, mouse_area, pin, responsive, rich_text, row, scrollable,
    space, stack, text, text_editor, tooltip,
};
use iced::{Center, Color, Element, Fill, Font, Length, Padding, Shadow, Size, Theme, Vector};
use phpthunder::inspection::Severity;
use phpthunder::symbol::Kind;

/// Shorthand for an element producing app `Message`s.
type El<'a> = Element<'a, Message>;

/// Width of the line-number gutter; also the x origin for popups and find marks.
pub(crate) const GUTTER_WIDTH: f32 = 56.0;
/// Matches the macOS title bar so the window buttons are vertically centered.
const TITLE_BAR_HEIGHT: f32 = 32.0;
/// Left space reserved for the macOS window buttons inside the title bar.
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHTS_INSET: f32 = 76.0;
/// Small left margin where no window buttons overlay the title bar.
#[cfg(not(target_os = "macos"))]
const TRAFFIC_LIGHTS_INSET: f32 = 6.0;
/// Height of a project tree, usages or find-result row.
pub(crate) const ROW_HEIGHT: f32 = 26.0;
/// Completion rows visible at once; the window follows the selection.
const COMPLETION_ROWS: usize = 8;
/// Height of one completion row.
const COMPLETION_ROW: f32 = 28.0;
/// Width of the completion list column.
const COMPLETION_LIST_WIDTH: f32 = 460.0;
/// Width of the completion details pane beside the list.
const COMPLETION_DOC_WIDTH: f32 = 300.0;
/// Width of the hover popup; also used to clamp it inside the editor.
pub(crate) const HOVER_WIDTH: f32 = 460.0;

/// Text at `size` px in `color`: the building block for labels in this module.
pub(crate) fn label<'a>(s: impl text::IntoFragment<'a>, size: f32, color: Color) -> text::Text<'a> {
    text(s).size(size).color(color)
}

/// A 1px horizontal rule. Public so the navigation palette (the `palette` module) can
/// separate its sections with the same line as the main layout.
pub fn rule<'a>() -> El<'a> {
    hsep()
}

/// A full-width 1px separator in `BORDER` color between stacked panes.
pub(crate) fn hsep<'a>() -> El<'a> {
    container(space())
        .width(Fill)
        .height(1)
        .style(fill(BORDER))
        .into()
}

/// A full-height 1px separator between side-by-side panes and tabs.
pub(crate) fn vsep<'a>() -> El<'a> {
    container(space())
        .width(1)
        .height(Fill)
        .style(fill(BORDER))
        .into()
}

/// Formats indexing time for the status bar: milliseconds with one decimal
/// below a second, seconds above.
fn format_elapsed(d: std::time::Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms < 1000.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{:.1} s", ms / 1000.0)
    }
}

impl App {
    /// Root view, called by iced after every update (wired in main.rs). Shows the
    /// welcome screen until a project is open; otherwise lays out title bar,
    /// rail, optional sidebar, main area and status bar, and stacks the palette
    /// or find-in-path dialog over a click-to-close backdrop when one is open.
    pub fn view(&self) -> El<'_> {
        if self.tree.root.is_none() {
            return self.welcome();
        }
        let mut body = row![self.rail()].spacing(ISLAND_GAP).padding(Padding {
            right: ISLAND_GAP,
            ..Padding::ZERO
        });
        if self.sidebar {
            body = body.push(self.sidebar());
        }
        body = body.push(self.main_area());
        let base = container(
            column![self.title_bar(), body.height(Fill), self.status_bar()]
                .width(Fill)
                .height(Fill),
        )
        .style(fill(CHROME));
        let modal: Option<El<'_>> = match (&self.palette, &self.search_dialog, &self.rename) {
            (Some(p), _, _) => Some(self.palette_view(p)),
            (None, Some(_), _) => Some(self.search_dialog_view()),
            (None, None, Some(_)) => Some(self.rename_dialog_view()),
            _ if self.name_prompt.is_some() => Some(self.name_prompt_view()),
            _ if self.refactor_prompt.is_some() => Some(self.refactor_prompt_view()),
            _ => None,
        };
        match modal {
            Some(m) => {
                let backdrop = mouse_area(container(space()).width(Fill).height(Fill).style(fill(
                    iced::Color {
                        a: 0.35,
                        ..iced::Color::BLACK
                    },
                )))
                .on_press(Message::CloseModal);
                stack![base, backdrop, m].width(Fill).height(Fill).into()
            }
            None => base.into(),
        }
    }

    /// Welcome screen shown before a project folder is open (see
    /// `ui/welcome.rs`). Keeps a draggable strip on top because the native
    /// title bar is hidden.
    fn welcome(&self) -> El<'_> {
        let content = self.welcome_body();
        let drag = mouse_area(container(space()).height(TITLE_BAR_HEIGHT).width(Fill))
            .on_press(Message::DragWindow)
            .on_double_click(Message::ToggleMaximize);
        column![drag, container(content).center(Fill)]
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Custom title bar: logo, project name and current git branch. Doubles as
    /// the window drag area (double-click maximizes) since the native title bar
    /// is transparent on macOS.
    fn title_bar(&self) -> El<'_> {
        let root = self
            .tree
            .root
            .as_deref()
            .map(crate::app::file_name)
            .unwrap_or_default();
        let initials: String = root
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .take(2)
            .filter_map(|w| w.chars().next())
            .collect::<String>()
            .to_uppercase();
        let project = button(
            row![
                container(label(initials, 10.0, TEXT_BRIGHT).font(UI_BOLD))
                    .width(20)
                    .height(20)
                    .center(20)
                    .style(|_: &Theme| container::Style {
                        background: Some(rgb(0x5C6E5A).into()),
                        border: iced::border::rounded(4),
                        ..Default::default()
                    }),
                label(root, 13.0, TEXT).font(UI_MEDIUM),
                icon(Icon::ChevronDown, 12.0, 2.0, MUTED),
            ]
            .spacing(8)
            .align_y(Center),
        )
        .padding([2, 6])
        .style(ghost_button)
        .on_press(Message::OpenFolderDialog);
        let mut bar = row![space().width(TRAFFIC_LIGHTS_INSET), project]
            .spacing(6)
            .align_y(Center)
            .padding(Padding {
                left: 0.0,
                right: 12.0,
                top: 0.0,
                bottom: 0.0,
            });
        if let Some(branch) = &self.branch {
            bar = bar.push(
                button(
                    row![
                        icon(Icon::Branch, 14.0, 2.0, MUTED),
                        label(branch.clone(), 13.0, TEXT),
                        icon(Icon::ChevronDown, 12.0, 2.0, MUTED),
                    ]
                    .spacing(6)
                    .align_y(Center),
                )
                .padding([2, 6])
                .style(ghost_button)
                .on_press(Message::GitShowBranches),
            );
        }
        bar = bar.push(space().width(Fill));
        if let Some(doc) = self.doc() {
            bar = bar.push(
                row![file_icon(&doc.path, 13.0), label(doc.title(), 13.0, TEXT)]
                    .spacing(2)
                    .align_y(Center),
            );
        }
        let tool = |which: Icon, color: Color, msg: Message, tip: String| -> El<'_> {
            tooltip(
                button(icon(which, 16.0, 2.0, color))
                    .padding(6)
                    .style(ghost_button)
                    .on_press(msg),
                container(label(tip, 12.0, TEXT))
                    .padding([4, 8])
                    .style(popup),
                tooltip::Position::Bottom,
            )
            .into()
        };
        bar = bar
            .push(tool(
                Icon::Resume,
                rgb(0x76B366),
                Message::RunTests(TestTarget::Context),
                format!("{}  {}", tr!("rail-run-tests"), hint(Cmd::RunTests)),
            ))
            .push(tool(
                Icon::Bug,
                rgb(0x76B366),
                Message::DebugTest,
                format!("{}  {}", tr!("debug-test"), hint(Cmd::DebugTest)),
            ))
            .push(tool(
                Icon::Search,
                TEXT_SOFT,
                Message::Command(Cmd::SearchEverywhere),
                format!(
                    "{}  {}",
                    tr!("action-search-everywhere"),
                    hint(Cmd::SearchEverywhere)
                ),
            ));
        let bar = container(bar)
            .height(TITLE_BAR_HEIGHT)
            .width(Fill)
            .center_y(TITLE_BAR_HEIGHT)
            .style(fill(CHROME));
        mouse_area(bar)
            .on_press(Message::DragWindow)
            .on_double_click(Message::ToggleMaximize)
            .into()
    }

    /// Narrow tool rail on the far left. Only the project button is wired (it
    /// toggles the sidebar); the others are placeholders with tooltips.
    fn rail(&self) -> El<'_> {
        let item = |which: Icon, active: bool, msg: Option<Message>, tip: String| -> El<'_> {
            let b = button(
                container(icon(which, 20.0, 1.8, if active { ACCENT } else { MUTED })).center(44),
            )
            .width(44)
            .height(44)
            .padding(0)
            .style(rail_button(active))
            .on_press_maybe(msg);
            tooltip(
                b,
                container(label(tip, 12.0, TEXT))
                    .padding([4, 8])
                    .style(popup),
                tooltip::Position::Right,
            )
            .into()
        };
        let rail = column![
            item(
                Icon::Folder,
                self.sidebar,
                Some(Message::ToggleSidebar),
                format!("{}  {}", tr!("rail-project"), hint(Cmd::ToggleSidebar))
            ),
            item(
                Icon::Search,
                false,
                Some(Message::Command(Cmd::FindInPath)),
                format!("{}  {}", tr!("action-find-in-path"), hint(Cmd::FindInPath))
            ),
            item(
                Icon::Symbols,
                false,
                Some(Message::Command(Cmd::GotoSymbol)),
                format!("{}  {}", tr!("action-go-to-symbol"), hint(Cmd::GotoSymbol))
            ),
            item(
                Icon::Tests,
                self.bottom == Some(Bottom::Tests),
                Some(Message::RunTests(TestTarget::Context)),
                format!("{}  {}", tr!("rail-run-tests"), hint(Cmd::RunTests))
            ),
            item(
                Icon::Terminal,
                self.bottom == Some(Bottom::Terminal),
                Some(Message::Command(Cmd::ToggleTerminal)),
                format!("{}  {}", tr!("panel-terminal"), hint(Cmd::ToggleTerminal))
            ),
            item(
                Icon::Sparkle,
                self.bottom == Some(Bottom::Claude),
                Some(Message::Command(Cmd::ToggleClaude)),
                format!("{}  {}", tr!("panel-claude"), hint(Cmd::ToggleClaude))
            ),
            item(
                Icon::Bug,
                self.bottom == Some(Bottom::Debug),
                Some(Message::DebugTest),
                format!("{}  {}", tr!("debug-test"), hint(Cmd::DebugTest))
            ),
            item(
                Icon::Commit,
                self.bottom == Some(Bottom::Commit),
                Some(Message::Command(Cmd::Commit)),
                format!("{}  {}", tr!("panel-commit"), hint(Cmd::Commit))
            ),
            space().height(Fill),
            container(logo(Logo::Mark, 22.0)).padding(8),
        ]
        .spacing(4)
        .height(Fill)
        .padding(Padding {
            top: 4.0,
            ..Padding::ZERO
        })
        .align_x(Center);
        container(rail)
            .width(48)
            .height(Fill)
            .style(fill(CHROME))
            .into()
    }

    /// Project tree panel: one row per visible entry from `tree.rows()`,
    /// indented by depth, with a chevron for directories and a colored dot for
    /// files. Clicking toggles a directory or opens a file; the active
    /// document's row is highlighted.
    fn sidebar(&self) -> El<'_> {
        let active = self.doc().map(|d| d.path.as_path());
        let rows = self.tree.rows().into_iter().map(|r| {
            let indent = space().width(4.0 + r.depth as f32 * 16.0);
            let chevron: El<'_> = if r.entry.is_dir {
                icon(
                    if r.expanded {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    },
                    12.0,
                    2.2,
                    MUTED,
                )
                .into()
            } else {
                space().width(12).into()
            };
            let marker: El<'_> = if r.entry.is_dir {
                folder_icon(&r.entry.path, self.tree.root.as_deref(), r.expanded, 15.0)
            } else {
                file_icon(&r.entry.path, 15.0)
            };
            let selected = active == Some(r.entry.path.as_path());
            let msg = if r.entry.is_dir {
                Message::ToggleDir(r.entry.path.clone())
            } else {
                Message::OpenFile(r.entry.path.clone())
            };
            let change = self.git.as_ref().and_then(|g| {
                if r.entry.is_dir {
                    g.files
                        .iter()
                        .any(|f| f.path.starts_with(&r.entry.path))
                        .then_some(crate::git::Change::Modified)
                } else {
                    g.change_of(&r.entry.path)
                }
            });
            let color = match change {
                Some(change) => change_color(change),
                None if selected => TEXT_BRIGHT,
                None if r.entry.is_dir => TEXT,
                None => TEXT_SOFT,
            };
            let row_button_el = button(
                row![
                    indent,
                    chevron,
                    marker,
                    label(r.entry.name.clone(), 13.0, color)
                ]
                .spacing(4)
                .align_y(Center),
            )
            .on_press(msg)
            .width(Fill)
            .height(ROW_HEIGHT)
            .padding(Padding {
                left: 0.0,
                right: 8.0,
                top: 0.0,
                bottom: 0.0,
            })
            .style(row_button(selected));
            mouse_area(row_button_el)
                .on_right_press(Message::ShowFileActions(r.entry.path.clone()))
                .into()
        });
        let new_file = self.tree.root.clone().map(|root| {
            tooltip(
                button(label("+", 16.0, MUTED))
                    .padding([0, 6])
                    .style(ghost_button)
                    .on_press(Message::ShowFileActions(root)),
                container(label(tr!("file-new-file"), 12.0, TEXT))
                    .padding([4, 8])
                    .style(popup),
                tooltip::Position::Bottom,
            )
        });
        let mut title = row![
            label(tr!("rail-project"), 13.0, TEXT).font(UI_BOLD),
            icon(Icon::ChevronDown, 12.0, 2.0, MUTED),
            space().width(Fill),
        ]
        .spacing(6)
        .align_y(Center);
        if let Some(button) = new_file {
            title = title.push(button);
        }
        let header = container(title).padding([0, 16]).height(40).center_y(40);
        let list = scrollable(column(rows).spacing(1).padding([0, 8]))
            .height(Fill)
            .style(scrollbars);
        container(column![header, list])
            .width(280)
            .height(Fill)
            .style(island)
            .clip(true)
            .into()
    }

    /// Everything right of the sidebar: tabs, breadcrumb, optional find bar,
    /// editor and optional bottom panel. With no open document it shows the
    /// shortcut cheat sheet instead.
    fn main_area(&self) -> El<'_> {
        let editor: El<'_> = match self.doc() {
            None => container(self.shortcuts())
                .center(Fill)
                .style(island)
                .into(),
            Some(doc) => {
                let mut col = column![self.tabs()];
                if let Some(bar) = self.find_bar_view() {
                    col = col.push(bar);
                }
                col = col.push(self.editor_area(doc));
                container(col.width(Fill).height(Fill))
                    .style(island)
                    .clip(true)
                    .into()
            }
        };
        let mut col = column![editor].spacing(ISLAND_GAP);
        if let Some(panel) = self.bottom_panel() {
            col = col.push(panel);
        }
        col.width(Fill).height(Fill).into()
    }

    /// Cheat sheet of navigation shortcuts for the empty editor area; key labels
    /// come from `keymap::hint`.
    fn shortcuts(&self) -> El<'_> {
        let line = |cmd: Cmd, what: String| {
            row![
                label(what, 13.0, MUTED).width(200),
                label(hint(cmd), 13.0, TEXT_SOFT).font(MONO)
            ]
            .spacing(16)
        };
        column![
            label(tr!("shortcuts-title"), 15.0, TEXT),
            space().height(8),
            line(Cmd::SearchEverywhere, tr!("action-search-everywhere")),
            line(Cmd::GotoFile, tr!("action-go-to-file")),
            line(Cmd::GotoClass, tr!("action-go-to-class")),
            line(Cmd::GotoSymbol, tr!("action-go-to-symbol")),
            line(Cmd::RecentFiles, tr!("action-recent-files")),
            line(Cmd::FindInPath, tr!("action-find-in-path")),
        ]
        .spacing(10)
        .into()
    }

    /// Tab strip of open documents. The active tab takes the editor background
    /// with an accent top line; unsaved documents get a `●` prefix. Scrolls
    /// horizontally without a visible scrollbar.
    fn tabs(&self) -> El<'_> {
        let tabs = self.docs.iter().enumerate().map(|(i, d)| -> El<'_> {
            let active = self.active == Some(i);
            let title = if d.dirty {
                format!("● {}", d.title())
            } else {
                d.title()
            };
            let close = button(icon(Icon::Close, 12.0, 2.0, MUTED))
                .padding(4)
                .style(ghost_button)
                .on_press(Message::CloseTab(i));
            let content = row![
                button(
                    row![
                        file_icon(&d.path, 13.0),
                        label(title, 13.0, if active { TEXT_BRIGHT } else { MUTED })
                    ]
                    .spacing(2)
                    .align_y(Center)
                )
                .padding(0)
                .style(|_, _| button::Style::default())
                .on_press(Message::SelectTab(i)),
                close,
            ]
            .spacing(4)
            .align_y(Center)
            .padding(Padding {
                left: 6.0,
                right: 4.0,
                top: 0.0,
                bottom: 0.0,
            });
            container(content)
                .height(28)
                .center_y(28)
                .style(move |_: &Theme| container::Style {
                    background: active.then(|| rgb(0x2C2C3B).into()),
                    border: if active {
                        iced::Border {
                            color: rgb(0x5C4A80),
                            width: 1.0,
                            radius: 6.0.into(),
                        }
                    } else {
                        iced::border::rounded(6)
                    },
                    ..Default::default()
                })
                .into()
        });
        let bar = row(tabs).spacing(4).align_y(Center);
        container(
            scrollable(bar.height(40)).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(0).scroller_width(0),
            )),
        )
        .width(Fill)
        .height(40)
        .padding([0, 8])
        .center_y(40)
        .into()
    }

    /// Breadcrumb for the status bar: project, the active file with its
    /// icon, then the class and member at the caret (`App::breadcrumb`).
    fn breadcrumb_row(&self) -> El<'_> {
        let mut r = row![].spacing(6).align_y(Center);
        if let Some(root) = self.tree.root.as_deref() {
            r = r.push(label(crate::app::file_name(root), 12.0, MUTED));
        }
        if let Some(doc) = self.doc() {
            r = r.push(label("›", 12.0, MUTED)).push(
                row![
                    file_icon(&doc.path, 12.0),
                    label(doc.title(), 12.0, TEXT_SOFT)
                ]
                .spacing(2)
                .align_y(Center),
            );
        }
        for part in self.breadcrumb() {
            r = r
                .push(label("›", 12.0, MUTED))
                .push(label(part, 12.0, MUTED));
        }
        r.into()
    }

    /// The editor for `doc`: line-number gutter and `text_editor` with PHP
    /// highlighting and the app's key bindings, with the caret-line band and
    /// find marks drawn underneath, all in one scrollable. Mouse moves feed the
    /// hover logic; popups are layered on top by `overlays`.
    fn editor_area<'a>(&'a self, doc: &'a Document) -> El<'a> {
        if let Some(preview) = self.file_preview(doc) {
            return preview;
        }
        let completion_open = self.completion.is_some();
        let intentions_open = self.intentions.is_some();
        let cursor_line = doc.cursor().line;

        let layout = self.wrap_layout(doc);
        let cursor_col = doc.visual_column(doc.cursor());
        let numbers: Vec<Span<'static, (), Font>> = (0..doc.line_count())
            .map(|i| {
                // Continuation rows of a wrapped line get no number.
                let breaks = "\n".repeat(layout.rows_of(i));
                Span::new(format!("{}{breaks}", i + 1)).color(if i == cursor_line {
                    TEXT
                } else {
                    GUTTER
                })
            })
            .collect();
        let gutter = container(
            rich_text(numbers)
                .font(MONO)
                .size(CODE_SIZE)
                .line_height(LineHeight::Absolute(CODE_LINE.into()))
                .align_x(Horizontal::Right)
                .width(Fill),
        )
        .width(GUTTER_WIDTH)
        .padding(Padding {
            top: EDITOR_PAD_TOP,
            right: 16.0,
            bottom: 0.0,
            left: 0.0,
        });

        let editor = text_editor(&doc.content)
            .id(EDITOR_ID)
            .on_action(Message::Edit)
            .font(MONO)
            .size(CODE_SIZE)
            .line_height(LineHeight::Absolute(CODE_LINE.into()))
            .padding(Padding {
                top: EDITOR_PAD_TOP,
                right: 24.0,
                bottom: 240.0,
                left: EDITOR_PAD_LEFT,
            })
            .height(Length::Shrink)
            .wrapping(if self.soft_wrap {
                Wrapping::Glyph
            } else {
                Wrapping::None
            })
            .highlight_with::<crate::editor::PhpHighlighter>(
                doc.highlight_settings(),
                crate::editor::format_token,
            )
            .key_binding(move |key| bindings(key, completion_open, intentions_open))
            .style(editor);

        let current_line = pin(container(space())
            .width(Fill)
            .height(CODE_LINE)
            .style(fill(CURRENT_LINE)))
        .y(crate::app::row_top(&layout, cursor_line, cursor_col))
        .width(Fill)
        .height(Fill);

        let mut content = Stack::new()
            .push(row![
                mouse_area(gutter)
                    .on_move(|p| Message::GutterMoved(p.y))
                    .on_press(Message::GutterPressed)
                    .interaction(iced::mouse::Interaction::Pointer),
                mouse_area(editor)
                    .on_move(Message::MouseMoved)
                    .on_exit(Message::MouseLeft)
            ])
            .push_under(current_line)
            .width(Fill);
        for mark in self.find_marks(doc) {
            content = content.push_under(mark);
        }
        for mark in self.diagnostic_marks(doc) {
            content = content.push_under(mark);
        }
        for mark in self.git_gutter_marks(doc) {
            content = content.push(mark);
        }
        for mark in self.debug_marks(doc) {
            content = content.push_under(mark);
        }
        for mark in self.extra_caret_marks(doc, &layout) {
            content = content.push(mark);
        }

        let scroller = scrollable(content)
            .id(EDITOR_SCROLL_ID)
            .on_scroll(Message::Scrolled)
            .width(Fill)
            .height(Fill)
            .style(scrollbars);

        let overlays = responsive(move |size| self.overlays(doc, size));
        iced::widget::sensor(stack![scroller, overlays].width(Fill).height(Fill))
            .on_show(Message::EditorResized)
            .on_resize(Message::EditorResized)
            .into()
    }

    /// Extra carets (bars) and their selections (tinted per line), drawn
    /// over the text; the main caret is the text editor's own.
    fn extra_caret_marks<'a>(
        &'a self,
        doc: &'a Document,
        layout: &crate::editor::WrapLayout,
    ) -> Vec<El<'a>> {
        let mut out: Vec<El<'a>> = Vec::new();
        for &(anchor, head) in &doc.extra_carets {
            let (start, end) = (anchor.min(head), anchor.max(head));
            if start < end {
                let (a, b) = (doc.position_of(start), doc.position_of(end));
                for line in a.line..=b.line {
                    let from = if line == a.line {
                        doc.visual_column(a)
                    } else {
                        0
                    };
                    let to = if line == b.line {
                        doc.visual_column(b)
                    } else {
                        doc.line_text(line).chars().count() + 1
                    };
                    out.push(
                        pin(container(space())
                            .width(((to.saturating_sub(from)).max(1)) as f32 * CHAR_WIDTH)
                            .height(CODE_LINE)
                            .style(fill(Color {
                                a: 0.35,
                                ..SELECTED_STRONG
                            })))
                        .x(crate::app::col_left(layout, from))
                        .y(crate::app::row_top(layout, line, from))
                        .into(),
                    );
                }
            }
            let pos = doc.position_of(head);
            let col = doc.visual_column(pos);
            out.push(
                pin(container(space())
                    .width(2)
                    .height(CODE_LINE)
                    .style(fill(TEXT_BRIGHT)))
                .x(crate::app::col_left(layout, col))
                .y(crate::app::row_top(layout, pos.line, col))
                .into(),
            );
        }
        out
    }

    /// Floating layers over the editor: the completion popup (or else the hover
    /// popup) anchored to its line and column, flipped above the line or clamped
    /// horizontally to stay inside `size`, plus a bottom-right toast for
    /// `notice`. Built inside `responsive` so the editor's size is known.
    fn overlays<'a>(&'a self, doc: &'a Document, size: Size) -> El<'a> {
        let mut layers = Stack::new().width(Fill).height(Fill);
        let scroll = doc.scroll_y;
        let layout = self.wrap_layout(doc);
        let top = |line: usize, col: usize| crate::app::row_top(&layout, line, col);
        let left = |col: usize| crate::app::col_left(&layout, col);
        if let Some(view) = &self.signature {
            // Above the caret line, so it never covers what is being typed.
            let y = (top(view.line, view.col) - scroll - 34.0).max(0.0);
            let x = (left(view.col) - 40.0).clamp(0.0, (size.width - 300.0).max(0.0));
            layers = layers.push(pin(signature_popup(view)).x(x).y(y));
        }
        if let Some(menu) = &self.intentions {
            let (popup, h) = intentions_popup(menu);
            let below = top(menu.line, menu.col) + CODE_LINE - scroll + 2.0;
            let above = top(menu.line, menu.col) - scroll - h - 2.0;
            let y = if below + h > size.height && above >= 0.0 {
                above
            } else {
                below
            };
            let x = (left(menu.col)).clamp(0.0, (size.width - INTENTIONS_WIDTH - 8.0).max(0.0));
            layers = layers.push(pin(popup).x(x).y(y));
        } else if let Some(c) = &self.completion {
            let (popup, w, h) = completion_popup(c);
            let below = top(c.line, c.col) + CODE_LINE - scroll + 2.0;
            let above = top(c.line, c.col) - scroll - h - 2.0;
            let y = if below + h > size.height && above >= 0.0 {
                above
            } else {
                below
            };
            let x = (left(c.col) - 36.0).clamp(0.0, (size.width - w - 8.0).max(0.0));
            layers = layers.push(pin(popup).x(x).y(y));
        } else if let Some(h) = &self.hover {
            let popup = hover_popup(h);
            let below = top(h.line, h.col) + CODE_LINE - scroll + 4.0;
            let x = (left(h.col) - 12.0).clamp(0.0, (size.width - HOVER_WIDTH - 8.0).max(0.0));
            let y = if below + 120.0 > size.height {
                (below - CODE_LINE - 130.0).max(0.0)
            } else {
                below
            };
            layers = layers.push(pin(popup).x(x).y(y));
        }
        if let Some(notice) = &self.notice {
            let toast = container(label(notice.clone(), 12.0, TEXT))
                .padding([8, 12])
                .style(popup);
            layers = layers.push(
                container(toast)
                    .align_x(Horizontal::Right)
                    .align_y(Vertical::Bottom)
                    .padding(16)
                    .width(Fill)
                    .height(Fill),
            );
        }
        layers.into()
    }

    /// Bottom tool panel: Usages and Find results, as tabs. Returns `None` when
    /// no panel is open; shown under the editor by `main_area`.
    fn bottom_panel(&self) -> Option<El<'_>> {
        let current = self.bottom?;
        let tab = |b: Bottom, name: String| -> El<'_> {
            let active = b == current;
            button(label(name, 13.0, if active { TEXT_BRIGHT } else { MUTED }))
                .on_press(Message::SelectBottom(b))
                .padding([0, 10])
                .height(33)
                .style(move |_: &Theme, _| button::Style {
                    background: Some(PANEL.into()),
                    shadow: if active {
                        Shadow {
                            color: ACCENT,
                            offset: Vector::new(0.0, 2.0),
                            blur_radius: 0.0,
                        }
                    } else {
                        Shadow::default()
                    },
                    text_color: TEXT,
                    ..Default::default()
                })
                .into()
        };
        let mut tabs = row![].spacing(4).align_y(Center);
        if self.usages.is_some() {
            tabs = tabs.push(tab(Bottom::Usages, tr!("panel-usages")));
        }
        if self.search.is_some() {
            tabs = tabs.push(tab(Bottom::Find, tr!("panel-find")));
        }
        tabs = tabs.push(tab(Bottom::Problems, tr!("panel-problems")));
        if self.hierarchy.is_some() {
            tabs = tabs.push(tab(Bottom::Hierarchy, tr!("panel-hierarchy")));
        }
        if self.tests.is_some() {
            tabs = tabs.push(tab(Bottom::Tests, tr!("panel-tests")));
        }
        tabs = tabs.push(tab(Bottom::Terminal, tr!("panel-terminal")));
        if self.claude.is_some() {
            tabs = tabs.push(tab(Bottom::Claude, tr!("panel-claude")));
        }
        if self.git.is_some() {
            tabs = tabs.push(tab(Bottom::Commit, tr!("panel-commit")));
        }
        if self.debug.is_some() {
            tabs = tabs.push(tab(Bottom::Debug, tr!("panel-debug")));
        }
        let close = button(icon(Icon::Close, 12.0, 2.0, MUTED))
            .padding(6)
            .style(ghost_button)
            .on_press(Message::CloseBottom);
        let header = container(
            row![tabs, space().width(Fill), close]
                .align_y(Center)
                .padding([0, 12]),
        )
        .height(34);
        let body = match current {
            Bottom::Usages => self.usages_body(),
            Bottom::Find => self.find_results_body(),
            Bottom::Problems => self.problems_body(),
            Bottom::Hierarchy => self.hierarchy_body(),
            Bottom::Terminal | Bottom::Claude => self.terminal_body(),
            Bottom::Tests => self.tests_body(),
            Bottom::Commit => self.commit_body(),
            Bottom::Debug => self.debug_body(),
        };
        Some(
            container(column![header, hsep(), body])
                .height(260)
                .width(Fill)
                .style(island)
                .clip(true)
                .into(),
        )
    }

    /// Usages tab body: a progress line while searching, then one clickable row
    /// per usage (location, code preview, `declaration` tag), selection
    /// highlighted.
    fn usages_body(&self) -> El<'_> {
        match &self.usages {
            None => space().into(),
            Some(Usages::Searching(name)) => container(label(
                tr!("usages-searching", name = name.as_str()),
                12.0,
                MUTED,
            ))
            .padding([10, 16])
            .into(),
            Some(Usages::Found {
                title,
                rows,
                selected,
            }) => {
                let list = rows.iter().enumerate().map(|(i, r)| {
                    let mut line = row![
                        label(r.label.clone(), 13.0, TEXT_SOFT).width(200),
                        label(r.preview.clone(), 13.0, rgb(0xB5BCCA))
                            .font(MONO)
                            .wrapping(Wrapping::None),
                    ]
                    .spacing(12)
                    .align_y(Center);
                    if r.is_declaration {
                        line = line.push(
                            container(label(tr!("usages-declaration"), 11.0, MUTED))
                                .padding([1, 6])
                                .style(pill),
                        );
                    }
                    button(line)
                        .on_press(Message::OpenUsage(i))
                        .width(Fill)
                        .height(ROW_HEIGHT)
                        .padding([0, 8])
                        .style(row_button(*selected == Some(i)))
                        .into()
                });
                column![
                    container(label(title.clone(), 12.0, MUTED)).padding(Padding {
                        top: 10.0,
                        left: 16.0,
                        right: 16.0,
                        bottom: 4.0
                    }),
                    scrollable(column(list).spacing(1).padding([2, 8]))
                        .height(Fill)
                        .style(scrollbars),
                ]
                .into()
            }
        }
    }

    /// Find tab body for find-in-path: a summary line, then hits grouped by
    /// file with line number and preview, the match highlighted. Clicking a hit
    /// opens it.
    fn find_results_body(&self) -> El<'_> {
        match &self.search {
            None => space().into(),
            Some(Search::Running(q)) => container(label(
                tr!("find-results-searching", query = q.as_str()),
                12.0,
                MUTED,
            ))
            .padding([10, 16])
            .into(),
            Some(Search::Done(results, selected)) => {
                let files = results.files.len();
                let mut title = tr!(
                    "find-results-title",
                    query = results.query.as_str(),
                    matches = results.total,
                    files = files,
                );
                if results.truncated {
                    title = format!("{title} {}", tr!("find-results-truncated"));
                }
                let root = self.tree.root.as_deref();
                let mut list = column![].spacing(1).padding([2, 8]);
                for (fi, file) in results.files.iter().enumerate() {
                    let rel = root
                        .and_then(|r| file.path.strip_prefix(r).ok())
                        .unwrap_or(&file.path);
                    let dir = rel
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    list = list.push(
                        container(
                            row![
                                label(crate::app::file_name(&file.path), 13.0, TEXT)
                                    .font(UI_MEDIUM),
                                label(dir, 12.0, MUTED),
                                label(format!("{}", file.hits.len()), 11.0, MUTED),
                            ]
                            .spacing(10)
                            .align_y(Center),
                        )
                        .padding([4, 8]),
                    );
                    for (hi, hit) in file.hits.iter().enumerate() {
                        let (a, b) = (hit.start - hit.preview_shift, hit.end - hit.preview_shift);
                        let p = &hit.preview;
                        let a = a.min(p.len());
                        let b = b.min(p.len()).max(a);
                        let spans: Vec<Span<'_, (), Font>> =
                            if p.is_char_boundary(a) && p.is_char_boundary(b) {
                                vec![
                                    Span::new(p[..a].to_string()).color(rgb(0xB5BCCA)),
                                    Span::new(p[a..b].to_string())
                                        .color(TEXT_BRIGHT)
                                        .background(rgb(0x3D3320)),
                                    Span::new(p[b..].to_string()).color(rgb(0xB5BCCA)),
                                ]
                            } else {
                                vec![Span::new(p.clone()).color(rgb(0xB5BCCA))]
                            };
                        let row = row![
                            label(format!("{}", hit.line + 1), 12.0, MUTED)
                                .font(MONO)
                                .width(48)
                                .align_x(Horizontal::Right),
                            rich_text(spans)
                                .font(MONO)
                                .size(13)
                                .wrapping(Wrapping::None),
                        ]
                        .spacing(12)
                        .align_y(Center);
                        list = list.push(
                            button(row)
                                .on_press(Message::OpenSearchHit(fi, hi))
                                .width(Fill)
                                .height(ROW_HEIGHT)
                                .padding([0, 8])
                                .style(row_button(*selected == Some((fi, hi)))),
                        );
                    }
                }
                let replacement = self
                    .last_search
                    .as_ref()
                    .map(|s| s.replacement.as_str())
                    .unwrap_or_default();
                let replace_label = if self.confirm_replace_in_path {
                    label(
                        tr!("replace-in-path-confirm", count = results.total),
                        12.0,
                        rgb(0xF7768E),
                    )
                } else {
                    label(tr!("replace-in-path"), 12.0, TEXT_SOFT)
                };
                let header = row![
                    label(title, 12.0, MUTED),
                    space().width(Fill),
                    iced::widget::text_input(&tr!("replace-placeholder"), replacement)
                        .on_input(Message::SearchReplaceText)
                        .size(12)
                        .font(MONO)
                        .padding([3, 8])
                        .width(260)
                        .style(boxed_input),
                    button(replace_label)
                        .padding([3, 10])
                        .style(ghost_button)
                        .on_press_maybe((results.total > 0).then_some(Message::ReplaceInPath)),
                ]
                .spacing(8)
                .align_y(Center);
                column![
                    container(header).padding(Padding {
                        top: 6.0,
                        left: 16.0,
                        right: 12.0,
                        bottom: 4.0
                    }),
                    scrollable(list).height(Fill).style(scrollbars),
                ]
                .into()
            }
        }
    }

    /// The find/replace bar above the editor; `None` when find is closed. Holds
    /// the query input, case/word/regex toggles, match count, previous/next
    /// buttons and an expandable replace row.
    fn find_bar_view(&self) -> Option<El<'_>> {
        let f = self.find.as_ref()?;
        let toggle = |t: Toggle, on: bool, text_: &'static str, tip: String| -> El<'_> {
            tooltip(
                button(label(text_, 12.0, if on { ACCENT } else { MUTED }).font(MONO))
                    .padding([3, 7])
                    .style(toggle_button(on))
                    .on_press(Message::FindToggle(t)),
                container(label(tip, 12.0, TEXT))
                    .padding([4, 8])
                    .style(popup),
                tooltip::Position::Bottom,
            )
            .into()
        };
        let count = match (&f.error, f.matches.len(), f.current) {
            (Some(_), _, _) => label(tr!("find-bad-pattern"), 12.0, rgb(0xF7768E)),
            (None, 0, _) if !f.query.is_empty() => {
                label(tr!("find-no-results"), 12.0, rgb(0xF7768E))
            }
            (None, 0, _) => label("", 12.0, MUTED),
            (None, n, Some(c)) => label(
                tr!("list-position", current = c + 1, total = n),
                12.0,
                MUTED,
            ),
            (None, n, None) => label(tr!("find-count", count = n), 12.0, MUTED),
        };
        let input = iced::widget::text_input(&tr!("find-placeholder"), &f.query)
            .id(iced::widget::Id::new(search::FIND_INPUT))
            .on_input(Message::FindQuery)
            .on_submit(Message::FindStep(true))
            .size(13)
            .font(MONO)
            .padding([5, 8])
            .width(320)
            .style(boxed_input);
        let nav = |forward: bool| -> El<'_> {
            button(label(if forward { "↓" } else { "↑" }, 13.0, TEXT_SOFT))
                .padding([2, 8])
                .style(ghost_button)
                .on_press(Message::FindStep(forward))
                .into()
        };
        let expand = button(icon(
            if f.show_replace {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            12.0,
            2.2,
            MUTED,
        ))
        .padding(4)
        .style(ghost_button)
        .on_press(Message::ToggleReplace);
        let mut find_row = row![
            expand,
            input,
            toggle(
                Toggle::Case,
                f.options.case_sensitive,
                "Aa",
                tr!("find-match-case")
            ),
            toggle(Toggle::Word, f.options.whole_word, "W", tr!("find-words")),
            toggle(Toggle::Regex, f.options.regex, ".*", tr!("find-regex")),
            count,
            nav(false),
            nav(true),
            space().width(Fill),
        ]
        .spacing(6)
        .align_y(Center);
        find_row = find_row.push(
            button(icon(Icon::Close, 12.0, 2.0, MUTED))
                .padding(6)
                .style(ghost_button)
                .on_press(Message::CloseFind),
        );
        let mut col = column![find_row].spacing(6);
        if f.show_replace {
            let replace = iced::widget::text_input(&tr!("replace-placeholder"), &f.replacement)
                .id(iced::widget::Id::new(search::REPLACE_INPUT))
                .on_input(Message::ReplaceText)
                .on_submit(Message::ReplaceOne)
                .size(13)
                .font(MONO)
                .padding([5, 8])
                .width(320)
                .style(boxed_input);
            let action = |name: String, msg: Message| -> El<'_> {
                button(label(name, 12.0, TEXT_SOFT))
                    .padding([4, 10])
                    .style(ghost_button)
                    .on_press(msg)
                    .into()
            };
            col = col.push(
                row![
                    space().width(20),
                    replace,
                    action(tr!("replace-one"), Message::ReplaceOne),
                    action(tr!("replace-all"), Message::ReplaceAll)
                ]
                .spacing(6)
                .align_y(Center),
            );
        }
        Some(
            container(col)
                .padding([6, 12])
                .width(Fill)
                .style(fill(PANEL))
                .into(),
        )
    }

    /// Highlight boxes for find matches visible in the viewport. Only lines
    /// between the scroll offset and the viewport bottom are scanned (at most
    /// 500 boxes); the current match is stronger. Pushed under the editor text
    /// by `editor_area`.
    fn find_marks<'a>(&'a self, doc: &'a Document) -> Vec<El<'a>> {
        let Some(f) = &self.find else {
            return Vec::new();
        };
        if f.matches.is_empty() {
            return Vec::new();
        }
        let layout = self.wrap_layout(doc);
        let (first_line, last_line) = self.visible_lines(doc, &layout);
        let from = doc.line_start(first_line);
        let to = doc.line_start(last_line + 1);
        let start = f.matches.partition_point(|&(s, _)| s < from);
        f.matches[start..]
            .iter()
            .enumerate()
            .take_while(|(_, m)| m.0 < to)
            .take(500)
            .map(|(i, &(s, e))| {
                let is_current = f.current == Some(start + i);
                let a = doc.position_of(s);
                let line_end = doc.line_end(a.line);
                let b = doc.position_of(e.min(line_end));
                let col = doc.visual_column(a);
                let x = crate::app::col_left(&layout, col);
                let w = ((doc.visual_column(b).saturating_sub(col)).max(1)) as f32 * CHAR_WIDTH;
                let w = if layout.wraps() {
                    w.min((self.wrap_columns() - layout.column_in_row(col)) as f32 * CHAR_WIDTH)
                } else {
                    w
                };
                let color = if is_current {
                    Color { a: 0.55, ..ACCENT }
                } else {
                    Color { a: 0.22, ..ACCENT }
                };
                pin(container(space())
                    .width(w)
                    .height(CODE_LINE - 2.0)
                    .style(move |_: &Theme| container::Style {
                        background: Some(color.into()),
                        border: iced::border::rounded(3),
                        ..Default::default()
                    }))
                .x(x)
                .y(crate::app::row_top(&layout, a.line, col) + 1.0)
                .into()
            })
            .collect()
    }

    /// The find-in-path dialog: query, case/word/regex/vendor toggles, file mask
    /// and a Find button. Shown as a modal by `view`.
    fn search_dialog_view(&self) -> El<'_> {
        let Some(d) = &self.search_dialog else {
            return space().into();
        };
        let toggle = |t: Toggle, on: bool, text_: &'static str| -> El<'_> {
            button(label(text_, 12.0, if on { ACCENT } else { MUTED }).font(MONO))
                .padding([3, 8])
                .style(toggle_button(on))
                .on_press(Message::SearchToggle(t))
                .into()
        };
        let query = iced::widget::text_input(&tr!("search-placeholder"), &d.query)
            .id(iced::widget::Id::new(search::SEARCH_INPUT))
            .on_input(Message::SearchQuery)
            .on_submit(Message::SearchSubmit)
            .size(14)
            .font(MONO)
            .padding([8, 10])
            .style(boxed_input);
        let mask = iced::widget::text_input(&tr!("search-mask-placeholder"), &d.mask)
            .on_input(Message::SearchMask)
            .on_submit(Message::SearchSubmit)
            .size(13)
            .padding([6, 10])
            .width(220)
            .style(boxed_input);
        let content = column![
            row![
                label(tr!("search-title"), 14.0, TEXT_BRIGHT).font(UI_BOLD),
                space().width(Fill),
                label(tr!("search-hints"), 11.0, MUTED)
            ]
            .align_y(Center),
            query,
            row![
                toggle(Toggle::Case, d.options.case_sensitive, "Aa"),
                toggle(Toggle::Word, d.options.whole_word, "W"),
                toggle(Toggle::Regex, d.options.regex, ".*"),
                toggle(Toggle::Vendor, d.include_vendor, "vendor"),
                space().width(Fill),
                mask,
            ]
            .spacing(6)
            .align_y(Center),
            row![
                space().width(Fill),
                button(label(tr!("search-submit"), 13.0, rgb(0x14161C)).font(UI_MEDIUM))
                    .padding([6, 16])
                    .style(primary_button)
                    .on_press(Message::SearchSubmit),
            ],
        ]
        .spacing(12)
        .padding(16)
        .width(640);
        container(container(content).style(popup))
            .width(Fill)
            .height(Fill)
            .align_x(Horizontal::Center)
            .padding(Padding {
                top: 90.0,
                ..Padding::ZERO
            })
            .into()
    }

    /// Bottom status bar: indexing progress or result on the left, and caret
    /// position, encoding and language of the active document on the right.
    fn status_bar(&self) -> El<'_> {
        let indexing = match &self.indexing {
            Indexing::Idle => String::new(),
            Indexing::Running => tr!("status-indexing"),
            Indexing::Done(r) => {
                tr!(
                    "status-indexed",
                    files = r.files,
                    elapsed = format_elapsed(r.elapsed)
                )
            }
        };
        let mut bar = row![
            self.breadcrumb_row(),
            space().width(Fill),
            row![
                icon(Icon::Bolt, 12.0, 2.4, MUTED),
                label(indexing, 12.0, MUTED)
            ]
            .spacing(6)
            .align_y(Center),
        ]
        .spacing(18)
        .align_y(Center)
        .padding([0, 12]);
        if let Some(doc) = self.doc().filter(|d| d.is_php()) {
            let (errors, warnings) = self.problem_counts();
            let count = |which: Icon, n: usize, severity: Severity| {
                let color = if n > 0 {
                    severity_color(severity)
                } else {
                    MUTED
                };
                row![
                    icon(which, 12.0, 2.0, color),
                    label(n.to_string(), 12.0, color)
                ]
                .spacing(4)
                .align_y(Center)
            };
            let counts = row![
                count(Icon::Error, errors, Severity::Error),
                count(Icon::Warning, warnings, Severity::Warning)
            ]
            .spacing(10);
            let stale = doc.diagnostics_version != doc.version();
            bar = bar.push(tooltip(
                button(counts)
                    .padding([2, 6])
                    .style(ghost_button)
                    .on_press(Message::Command(Cmd::Problems)),
                container(label(
                    if stale {
                        tr!("status-inspecting")
                    } else {
                        format!("{}  {}", tr!("panel-problems"), hint(Cmd::Problems))
                    },
                    12.0,
                    TEXT,
                ))
                .padding([4, 8])
                .style(popup),
                tooltip::Position::Top,
            ));
        }
        if let Some(doc) = self.doc() {
            let pos = doc.cursor();
            bar = bar
                .push(label(
                    tr!(
                        "status-position",
                        line = pos.line + 1,
                        column = doc.visual_column(pos) + 1
                    ),
                    12.0,
                    MUTED,
                ))
                .push(label("UTF-8", 12.0, MUTED));
            bar = bar.push(label(doc.file_type.name(), 12.0, MUTED));
        }
        container(bar)
            .height(26)
            .center_y(26)
            .width(Fill)
            .style(fill(CHROME))
            .into()
    }
}

/// Colored one-letter badge for a completion item's symbol kind.
fn kind_badge<'a>(kind: Kind) -> El<'a> {
    let (letter, colors) = match kind {
        Kind::Method => ("m", BADGE_METHOD),
        Kind::Function => ("f", BADGE_METHOD),
        Kind::Property => ("p", BADGE_PROPERTY),
        Kind::Class | Kind::Trait => ("C", BADGE_CLASS),
        Kind::Interface => ("I", BADGE_CLASS),
        Kind::Enum => ("E", BADGE_CLASS),
        Kind::Constant | Kind::ClassConstant | Kind::EnumCase => ("k", BADGE_OTHER),
    };
    container(label(letter, 11.0, colors.1).font(MONO))
        .center(18)
        .style(badge(colors))
        .into()
}

/// Parameter list and result type of a proposal, from its detail: a
/// signature gives `(int $a)` and its return type, a property its declared
/// type; other proposals (classes, variables, keywords) give neither.
fn signature_parts(candidate: &phpthunder::analysis::Candidate) -> (String, String) {
    let detail = candidate.detail.as_str();
    match candidate.kind {
        Kind::Method | Kind::Function => {
            let Some(open) = detail.find('(') else {
                return (String::new(), String::new());
            };
            let mut depth = 0usize;
            let mut close = detail.len();
            for (i, ch) in detail[open..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = open + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let params = detail[open..close.min(detail.len())].to_string();
            let ret = detail[close.min(detail.len())..]
                .trim()
                .trim_start_matches(':')
                .trim()
                .to_string();
            (params, ret)
        }
        Kind::Property if detail != "variable" => {
            let ty: Vec<&str> = detail
                .split_whitespace()
                .take_while(|w| !w.starts_with('$'))
                .filter(|w| {
                    !matches!(
                        *w,
                        "public"
                            | "protected"
                            | "private"
                            | "static"
                            | "readonly"
                            | "var"
                            | "@property"
                            | "@property-read"
                    )
                })
                .collect();
            (String::new(), ty.join(" "))
        }
        _ => (String::new(), String::new()),
    }
}

/// Longest parameter list shown after a proposal's name.
const MAX_TAIL_CHARS: usize = 44;

/// `text` cut to `max` characters with an ellipsis.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        format!("{}…", text.chars().take(max - 1).collect::<String>())
    } else {
        text.to_string()
    }
}

/// The proposal's label with the matched letters in bold accent color.
fn highlighted_label<'a>(item: &phpthunder::analysis::Ranked, selected: bool) -> El<'a> {
    let text_color = if selected { TEXT_BRIGHT } else { TEXT };
    let label_text = &item.candidate.label;
    let mut spans: Vec<Span<'static, (), Font>> = Vec::new();
    let mut at = 0;
    for range in &item.fragments {
        if range.start > at {
            spans.push(Span::new(label_text[at..range.start].to_string()).color(text_color));
        }
        spans.push(
            Span::new(label_text[range.clone()].to_string())
                .color(ACCENT)
                .font(MONO_BOLD),
        );
        at = range.end;
    }
    if at < label_text.len() {
        spans.push(Span::new(label_text[at..].to_string()).color(text_color));
    }
    rich_text(spans).font(MONO).size(13).into()
}

/// The completion list (a window of rows around the selection) plus a
/// details pane. Each row shows a kind badge, the label with its matched
/// letters highlighted, the parameter list, and the result type (or the
/// namespace of a class to import) on the right. Returns the popup and its
/// size, which `overlays` needs to place it above or below the caret line.
fn completion_popup(c: &Completion) -> (El<'_>, f32, f32) {
    let first = c.selected.saturating_sub(COMPLETION_ROWS - 1);
    let rows = c
        .items
        .iter()
        .enumerate()
        .skip(first)
        .take(COMPLETION_ROWS)
        .map(|(i, item)| {
            let selected = i == c.selected;
            let candidate = &item.candidate;
            let (params, ret) = signature_parts(candidate);
            let right = match &candidate.import {
                Some(_) => candidate
                    .detail
                    .rsplit_once('\\')
                    .map(|(ns, _)| ns)
                    .unwrap_or("")
                    .to_string(),
                None => ret,
            };
            let badge: El<'_> = if candidate.detail == phpthunder::analysis::KEYWORD_DETAIL {
                container(label("kw", 10.0, SYN_KEYWORD).font(MONO))
                    .center(18)
                    .style(badge(BADGE_OTHER))
                    .into()
            } else {
                kind_badge(candidate.kind)
            };
            let r = row![
                badge,
                row![
                    highlighted_label(item, selected),
                    label(shorten(&params, MAX_TAIL_CHARS), 12.0, MUTED)
                        .font(MONO)
                        .wrapping(Wrapping::None)
                ]
                .spacing(0)
                .align_y(Center)
                .width(Fill)
                .clip(true),
                label(right, 11.0, MUTED),
            ]
            .spacing(10)
            .align_y(Center);
            button(r)
                .on_press(Message::CompletionPick(i))
                .width(Fill)
                .height(COMPLETION_ROW)
                .padding([0, 8])
                .style(move |_: &Theme, status| button::Style {
                    background: match (selected, status) {
                        (true, _) => Some(SELECTED_STRONG.into()),
                        (false, button::Status::Hovered) => Some(SELECTED.into()),
                        _ => None,
                    },
                    text_color: TEXT,
                    border: iced::border::rounded(5),
                    ..Default::default()
                })
                .into()
        });
    let shown = c.items.len().min(COMPLETION_ROWS);
    let list = column(rows).width(COMPLETION_LIST_WIDTH).padding(4);
    let height = shown as f32 * COMPLETION_ROW + 8.0 + 2.0;
    let item = &c.items[c.selected].candidate;
    let details = column![
        label(item.detail.clone(), 12.0, SYN_VARIABLE).font(MONO),
        space().height(Fill),
        label(
            tr!(
                "list-position",
                current = c.selected + 1,
                total = c.items.len()
            ),
            11.0,
            MUTED
        ),
    ]
    .spacing(8)
    .padding([12, 14])
    .width(COMPLETION_DOC_WIDTH)
    .height(Fill);
    let body = row![
        list,
        container(space()).width(1).height(Fill).style(fill(BORDER)),
        details
    ]
    .height(height - 2.0);
    let popup = container(body).style(popup).width(Length::Shrink);
    (
        popup.into(),
        COMPLETION_LIST_WIDTH + COMPLETION_DOC_WIDTH + 3.0,
        height,
    )
}

/// Hover card for the symbol under the mouse: signature, doc text (if any)
/// and declaration location. Placed by `overlays`.
fn hover_popup(h: &Hover) -> El<'_> {
    let mut col = column![].spacing(8);
    for (severity, message) in &h.problems {
        col = col.push(label(message.clone(), 13.0, severity_color(*severity)));
    }
    if h.signature.is_empty() {
        return container(col)
            .padding([10, 14])
            .width(HOVER_WIDTH)
            .style(popup)
            .into();
    }
    col = col.push(label(h.signature.clone(), 13.0, SYN_VARIABLE).font(MONO));
    if !h.doc.is_empty() {
        col = col.push(label(h.doc.clone(), 13.0, rgb(0xB5BCCA)));
    }
    col = col.push(label(h.location.clone(), 12.0, MUTED));
    container(col)
        .padding([12, 14])
        .width(HOVER_WIDTH)
        .style(popup)
        .into()
}
