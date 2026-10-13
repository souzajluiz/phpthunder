//! The Thunder Dark layout: title bar, tool rail, project tree, tabs,
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
const COMPLETION_LIST_WIDTH: f32 = 340.0;
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
fn hsep<'a>() -> El<'a> {
    container(space())
        .width(Fill)
        .height(1)
        .style(fill(BORDER))
        .into()
}

/// A full-height 1px separator between side-by-side panes and tabs.
fn vsep<'a>() -> El<'a> {
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

/// Color of the file dot in the project tree: indigo for `.php`, muted otherwise.
fn file_color(name: &str) -> Color {
    if name.ends_with(".php") {
        rgb(0x8892D6)
    } else {
        MUTED
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
        let mut body = row![self.rail(), vsep()];
        if self.sidebar {
            body = body.push(self.sidebar()).push(vsep());
        }
        body = body.push(self.main_area());
        let base = column![
            self.title_bar(),
            hsep(),
            body.height(Fill),
            hsep(),
            self.status_bar()
        ]
        .width(Fill)
        .height(Fill);
        let modal: Option<El<'_>> = match (&self.palette, &self.search_dialog, &self.rename) {
            (Some(p), _, _) => Some(self.palette_view(p)),
            (None, Some(_), _) => Some(self.search_dialog_view()),
            (None, None, Some(_)) => Some(self.rename_dialog_view()),
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

    /// Welcome screen shown before a project folder is open: logo and an "Open
    /// Folder" button. Keeps a draggable strip on top because the native title
    /// bar is hidden.
    fn welcome(&self) -> El<'_> {
        let content = column![
            logo(Logo::Lockup, 220.0),
            label(tr!("welcome-hint"), 14.0, MUTED),
            button(label(tr!("welcome-open-folder"), 14.0, rgb(0x14161C)).font(UI_MEDIUM))
                .padding([10, 20])
                .style(primary_button)
                .on_press(Message::OpenFolderDialog),
        ]
        .spacing(16)
        .align_x(Center);
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
        let mut bar = row![
            space().width(TRAFFIC_LIGHTS_INSET),
            logo(Logo::Mark, 20.0),
            label(crate::APP_NAME, 13.0, TEXT).font(UI_BOLD),
            label(root, 13.0, MUTED),
            space().width(Fill),
        ]
        .spacing(10)
        .align_y(Center)
        .padding(Padding {
            left: 0.0,
            right: 16.0,
            top: 0.0,
            bottom: 0.0,
        });
        if let Some(branch) = &self.branch {
            bar = bar.push(
                row![
                    icon(Icon::Branch, 14.0, 2.0, MUTED),
                    label(branch.clone(), 13.0, MUTED)
                ]
                .spacing(6)
                .align_y(Center),
            );
        }
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
            item(Icon::Branch, false, None, tr!("rail-git")),
        ]
        .spacing(4)
        .padding(Padding {
            top: 8.0,
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
            let indent = space().width(8.0 + r.depth as f32 * 16.0);
            let marker: El<'_> = if r.entry.is_dir {
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
                container(space())
                    .width(8)
                    .height(8)
                    .style(move |_: &Theme| container::Style {
                        background: Some(file_color(&r.entry.name).into()),
                        border: iced::border::rounded(2),
                        ..Default::default()
                    })
                    .into()
            };
            let selected = active == Some(r.entry.path.as_path());
            let msg = if r.entry.is_dir {
                Message::ToggleDir(r.entry.path.clone())
            } else {
                Message::OpenFile(r.entry.path.clone())
            };
            let color = if selected {
                TEXT_BRIGHT
            } else if r.entry.is_dir {
                TEXT
            } else {
                TEXT_SOFT
            };
            button(
                row![
                    indent,
                    container(marker).width(14).center_x(14),
                    label(r.entry.name.clone(), 13.0, color)
                ]
                .spacing(6)
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
            .style(row_button(selected))
            .into()
        });
        let header = container(label(tr!("sidebar-project"), 11.0, MUTED).font(UI_BOLD))
            .padding([0, 16])
            .height(36)
            .center_y(36);
        let list = scrollable(column(rows).spacing(1).padding([0, 8]))
            .height(Fill)
            .style(scrollbars);
        container(column![header, list])
            .width(260)
            .height(Fill)
            .style(fill(PANEL))
            .into()
    }

    /// Everything right of the sidebar: tabs, breadcrumb, optional find bar,
    /// editor and optional bottom panel. With no open document it shows the
    /// shortcut cheat sheet instead.
    fn main_area(&self) -> El<'_> {
        let Some(doc) = self.doc() else {
            let mut col = column![container(self.shortcuts()).center(Fill).style(fill(BG))];
            if let Some(panel) = self.bottom_panel() {
                col = col.push(hsep()).push(panel);
            }
            return col.width(Fill).height(Fill).into();
        };
        let mut col = column![
            self.tabs(),
            hsep(),
            self.breadcrumb_bar(),
            container(space())
                .height(1)
                .width(Fill)
                .style(fill(rgb(0x191D25)))
        ];
        if let Some(bar) = self.find_bar_view() {
            col = col.push(bar).push(hsep());
        }
        col = col.push(self.editor_area(doc));
        if let Some(panel) = self.bottom_panel() {
            col = col.push(hsep()).push(panel);
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
                button(label(title, 13.0, if active { TEXT_BRIGHT } else { MUTED }))
                    .padding(0)
                    .style(|_, _| button::Style::default())
                    .on_press(Message::SelectTab(i)),
                close,
            ]
            .spacing(8)
            .align_y(Center)
            .padding(Padding {
                left: 16.0,
                right: 8.0,
                top: 0.0,
                bottom: 0.0,
            });
            container(content)
                .height(38)
                .center_y(38)
                .style(move |_: &Theme| container::Style {
                    background: Some(if active { BG } else { PANEL }.into()),
                    shadow: if active {
                        Shadow {
                            color: ACCENT,
                            offset: Vector::new(0.0, -2.0),
                            blur_radius: 0.0,
                        }
                    } else {
                        Shadow::default()
                    },
                    ..Default::default()
                })
                .into()
        });
        let mut bar = row![];
        for tab in tabs {
            bar = bar.push(tab).push(vsep());
        }
        container(
            scrollable(bar.height(38)).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(0).scroller_width(0),
            )),
        )
        .width(Fill)
        .height(38)
        .padding(Padding {
            top: 2.0,
            ..Padding::ZERO
        })
        .style(fill(PANEL))
        .into()
    }

    /// Breadcrumb under the tabs (from `App::breadcrumb`), last segment emphasized.
    fn breadcrumb_bar(&self) -> El<'_> {
        let parts = self.breadcrumb();
        let last = parts.len().saturating_sub(1);
        let mut r = row![].spacing(6).align_y(Center);
        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                r = r.push(label("›", 12.0, MUTED));
            }
            r = r.push(label(part, 12.0, if i == last { TEXT } else { MUTED }));
        }
        container(r)
            .height(28)
            .center_y(28)
            .padding([0, 16])
            .width(Fill)
            .into()
    }

    /// The editor for `doc`: line-number gutter and `text_editor` with PHP
    /// highlighting and the app's key bindings, with the caret-line band and
    /// find marks drawn underneath, all in one scrollable. Mouse moves feed the
    /// hover logic; popups are layered on top by `overlays`.
    fn editor_area<'a>(&'a self, doc: &'a Document) -> El<'a> {
        let completion_open = self.completion.is_some();
        let intentions_open = self.intentions.is_some();
        let cursor_line = doc.cursor().line;

        let numbers: Vec<Span<'static, (), Font>> = (0..doc.line_count())
            .map(|i| {
                Span::new(format!("{}\n", i + 1)).color(if i == cursor_line {
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
            .wrapping(Wrapping::None)
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
        .y(EDITOR_PAD_TOP + cursor_line as f32 * CODE_LINE)
        .width(Fill)
        .height(Fill);

        let mut content = Stack::new()
            .push(row![
                gutter,
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

        let scroller = scrollable(content)
            .id(EDITOR_SCROLL_ID)
            .on_scroll(Message::Scrolled)
            .width(Fill)
            .height(Fill)
            .style(scrollbars);

        let overlays = responsive(move |size| self.overlays(doc, size));
        stack![scroller, overlays].width(Fill).height(Fill).into()
    }

    /// Floating layers over the editor: the completion popup (or else the hover
    /// popup) anchored to its line and column, flipped above the line or clamped
    /// horizontally to stay inside `size`, plus a bottom-right toast for
    /// `notice`. Built inside `responsive` so the editor's size is known.
    fn overlays<'a>(&'a self, doc: &'a Document, size: Size) -> El<'a> {
        let mut layers = Stack::new().width(Fill).height(Fill);
        let scroll = doc.scroll_y;
        if let Some(view) = &self.signature {
            // Above the caret line, so it never covers what is being typed.
            let y = (EDITOR_PAD_TOP + view.line as f32 * CODE_LINE - scroll - 34.0).max(0.0);
            let x = (GUTTER_WIDTH + EDITOR_PAD_LEFT + view.col as f32 * CHAR_WIDTH - 40.0)
                .clamp(0.0, (size.width - 300.0).max(0.0));
            layers = layers.push(pin(signature_popup(view)).x(x).y(y));
        }
        if let Some(menu) = &self.intentions {
            let (popup, h) = intentions_popup(menu);
            let below = EDITOR_PAD_TOP + (menu.line + 1) as f32 * CODE_LINE - scroll + 2.0;
            let above = EDITOR_PAD_TOP + menu.line as f32 * CODE_LINE - scroll - h - 2.0;
            let y = if below + h > size.height && above >= 0.0 {
                above
            } else {
                below
            };
            let x = (GUTTER_WIDTH + EDITOR_PAD_LEFT + menu.col as f32 * CHAR_WIDTH)
                .clamp(0.0, (size.width - INTENTIONS_WIDTH - 8.0).max(0.0));
            layers = layers.push(pin(popup).x(x).y(y));
        } else if let Some(c) = &self.completion {
            let (popup, w, h) = completion_popup(c);
            let below = EDITOR_PAD_TOP + (c.line + 1) as f32 * CODE_LINE - scroll + 2.0;
            let above = EDITOR_PAD_TOP + c.line as f32 * CODE_LINE - scroll - h - 2.0;
            let y = if below + h > size.height && above >= 0.0 {
                above
            } else {
                below
            };
            let x = (GUTTER_WIDTH + EDITOR_PAD_LEFT + c.col as f32 * CHAR_WIDTH - 36.0)
                .clamp(0.0, (size.width - w - 8.0).max(0.0));
            layers = layers.push(pin(popup).x(x).y(y));
        } else if let Some(h) = &self.hover {
            let popup = hover_popup(h);
            let below = EDITOR_PAD_TOP + (h.line + 1) as f32 * CODE_LINE - scroll + 4.0;
            let x = (GUTTER_WIDTH + EDITOR_PAD_LEFT + h.col as f32 * CHAR_WIDTH - 12.0)
                .clamp(0.0, (size.width - HOVER_WIDTH - 8.0).max(0.0));
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
            Bottom::Terminal => self.terminal_body(),
            Bottom::Tests => self.tests_body(),
        };
        Some(
            container(column![header, hsep(), body])
                .height(230)
                .width(Fill)
                .style(fill(PANEL))
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
                column![
                    container(label(title, 12.0, MUTED)).padding(Padding {
                        top: 10.0,
                        left: 16.0,
                        right: 16.0,
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
        let first_line = ((doc.scroll_y - EDITOR_PAD_TOP) / CODE_LINE)
            .floor()
            .max(0.0) as usize;
        let last_line = first_line + (self.viewport_height / CODE_LINE).ceil() as usize + 1;
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
                let x = GUTTER_WIDTH + EDITOR_PAD_LEFT + doc.visual_column(a) as f32 * CHAR_WIDTH;
                let w = ((doc.visual_column(b).saturating_sub(doc.visual_column(a))).max(1)) as f32
                    * CHAR_WIDTH;
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
                .y(EDITOR_PAD_TOP + a.line as f32 * CODE_LINE + 1.0)
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
            row![
                icon(Icon::Bolt, 12.0, 2.4, ACCENT),
                label(indexing, 12.0, ACCENT)
            ]
            .spacing(6)
            .align_y(Center),
            space().width(Fill),
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
            if doc.is_php() {
                bar = bar.push(label("PHP", 12.0, MUTED));
            } else if doc.is_blade() {
                bar = bar.push(label("Blade", 12.0, MUTED));
            }
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

/// The completion list (a window of rows around the selection) plus a
/// details pane. Returns the popup and its size, which `overlays` needs to
/// place it above or below the caret line.
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
            let hint = item.import.as_ref().map(|_| {
                item.detail
                    .rsplit_once('\\')
                    .map(|(ns, _)| ns)
                    .unwrap_or("")
                    .to_string()
            });
            let mut r = row![
                kind_badge(item.kind),
                label(
                    item.label.clone(),
                    13.0,
                    if selected { TEXT_BRIGHT } else { TEXT }
                )
                .font(MONO)
                .width(Fill),
            ]
            .spacing(10)
            .align_y(Center);
            if let Some(ns) = hint {
                r = r.push(label(ns, 11.0, MUTED));
            }
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
    let item = &c.items[c.selected];
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
        container(space())
            .width(1)
            .height(Fill)
            .style(fill(rgb(0x262B36))),
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
