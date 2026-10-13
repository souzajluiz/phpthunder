//! Building and drawing the navigation popup's items.

use super::*;
use crate::app::{App, Message};
use crate::ui::*;
use iced::widget::text::Span;
use iced::widget::{Id, button, column, container, rich_text, row, space, text, text_input};
use iced::{Center, Element, Fill, Font, Length, Padding, Theme};
use phpthunder::symbol::{Kind, Symbol};
use std::path::{Path, PathBuf};

/// Displays `path` relative to the project `root` when inside it, else in
/// full. Used for file details and as the fuzzy-match fallback key.
fn rel(root: Option<&Path>, path: &Path) -> String {
    root.and_then(|r| path.strip_prefix(r).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Whether `path` lies under a `vendor` directory; such results get a score
/// penalty so project code ranks first.
fn is_vendor(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "vendor")
}

/// Kind letter for symbol items; matches the completion badges in the workbench view.
fn badge(kind: Kind) -> &'static str {
    match kind {
        Kind::Class | Kind::Trait => "C",
        Kind::Interface => "I",
        Kind::Enum => "E",
        Kind::Function => "f",
        Kind::Method => "m",
        Kind::Property => "p",
        Kind::Constant | Kind::ClassConstant | Kind::EnumCase => "k",
    }
}

impl App {
    /// Ranks the candidates for `mode` against `query` and returns the best
    /// `MAX_ITEMS`. Called from the `app` command handlers when the palette opens and on each
    /// query change. Symbol modes return nothing for an empty query (they would
    /// walk the whole index); Recent keeps most-recent-first order when empty;
    /// Line yields one item for the active document, clamped to its line count.
    pub fn palette_items(&self, mode: Mode, query: &str) -> Vec<Item> {
        let root = self.tree.root.as_deref();
        let query = query.trim();
        let mut scored: Vec<(i32, Item)> = Vec::new();
        let files = |scored: &mut Vec<(i32, Item)>,
                     paths: &mut dyn Iterator<Item = &PathBuf>,
                     boost: i32| {
            for path in paths {
                let name = crate::app::file_name(path);
                let relative = rel(root, path);
                // Match the name first; fall back to the whole relative path.
                let hit = fuzzy(query, &name)
                    .map(|(s, m)| (s + 10, m))
                    .or_else(|| fuzzy(query, &relative).map(|(s, _)| (s, Vec::new())));
                if let Some((s, matched)) = hit {
                    let penalty = if is_vendor(path) { 25 } else { 0 };
                    scored.push((
                        s + boost - penalty,
                        Item {
                            badge: "▤",
                            title: name,
                            matched,
                            detail: relative,
                            target: Target::File(path.clone()),
                        },
                    ));
                }
            }
        };
        let symbols = |scored: &mut Vec<(i32, Item)>, filter: &dyn Fn(&Symbol) -> bool| {
            if query.is_empty() {
                return;
            }
            self.lang.for_each_symbol(|path, s| {
                if !filter(s) {
                    return;
                }
                let title = if s.kind.is_member() {
                    let owner = s.fqn.rsplit('\\').next().unwrap_or(&s.fqn);
                    format!("{owner}::{}", s.name)
                } else {
                    s.fqn.rsplit('\\').next().unwrap_or(&s.fqn).to_string()
                };
                let key = if s.kind.is_member() { &s.name } else { &title };
                let Some((score, _)) = fuzzy(query, key) else {
                    return;
                };
                let matched = fuzzy(query, &title).map(|(_, m)| m).unwrap_or_default();
                let penalty = if is_vendor(path) { 25 } else { 0 };
                let ns = if s.kind.is_member() {
                    String::new()
                } else {
                    s.fqn
                        .rsplit_once('\\')
                        .map(|(n, _)| format!("{n} · "))
                        .unwrap_or_default()
                };
                scored.push((
                    score - penalty,
                    Item {
                        badge: badge(s.kind),
                        title,
                        matched,
                        detail: format!(
                            "{ns}{}:{}",
                            crate::app::file_name(path),
                            s.name_span.start.line + 1
                        ),
                        target: Target::Symbol(path.to_path_buf(), s.name_span.start),
                    },
                ));
            });
        };
        match mode {
            Mode::Files => files(&mut scored, &mut self.project_files.iter(), 0),
            Mode::Recent => {
                let n = self.recent.len() as i32;
                files(&mut scored, &mut self.recent.iter(), 0);
                // Keep most-recent-first order when the query is empty.
                for (i, (s, _)) in scored.iter_mut().enumerate() {
                    *s += n - i as i32;
                }
            }
            Mode::Classes => symbols(&mut scored, &|s| s.kind.is_class_like()),
            Mode::Symbols => symbols(&mut scored, &|s| !s.is_virtual),
            Mode::Everywhere => {
                files(&mut scored, &mut self.project_files.iter(), -5);
                symbols(&mut scored, &|s| !s.is_virtual);
            }
            Mode::Line => {
                let mut parts = query.split(':');
                let line = parts.next().and_then(|l| l.trim().parse::<usize>().ok());
                let col = parts
                    .next()
                    .and_then(|c| c.trim().parse::<usize>().ok())
                    .unwrap_or(1);
                if let (Some(line), Some(doc)) = (line, self.doc()) {
                    let line = line.clamp(1, doc.line_count().max(1));
                    scored.push((
                        0,
                        Item {
                            badge: "#",
                            title: format!("Line {line}, column {col}"),
                            matched: Vec::new(),
                            detail: doc.line_text(line - 1).trim().chars().take(80).collect(),
                            target: Target::Line(line - 1, col.saturating_sub(1)),
                        },
                    ));
                }
            }
        }
        if mode != Mode::Recent || !query.is_empty() {
            scored.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| a.1.title.len().cmp(&b.1.title.len()))
            });
        } else {
            scored.sort_by_key(|item| std::cmp::Reverse(item.0));
        }
        scored.into_iter().take(MAX_ITEMS).map(|(_, i)| i).collect()
    }

    /// Renders the palette as a centered modal card: query input, a window of
    /// `VISIBLE_ROWS` results around the selection with matched characters in
    /// the accent color, and a footer with the mode and keys. Called from
    /// `App::view`, which stacks it over a click-to-close backdrop.
    pub fn palette_view<'a>(&'a self, p: &'a Palette) -> Element<'a, Message> {
        let input = text_input(p.mode.placeholder(), &p.query)
            .id(Id::new(INPUT_ID))
            .on_input(Message::PaletteQuery)
            .on_submit(Message::PaletteSubmit)
            .size(14)
            .padding(Padding {
                top: 12.0,
                bottom: 12.0,
                left: 14.0,
                right: 14.0,
            })
            .style(input);
        let first = p.selected.saturating_sub(VISIBLE_ROWS - 1);
        let rows = p
            .items
            .iter()
            .enumerate()
            .skip(first)
            .take(VISIBLE_ROWS)
            .map(|(i, item)| {
                let selected = i == p.selected;
                let spans: Vec<Span<'_, (), Font>> = item
                    .title
                    .chars()
                    .enumerate()
                    .map(|(ci, ch)| {
                        let hit = item.matched.contains(&ci);
                        Span::new(ch.to_string()).color(if hit {
                            ACCENT
                        } else if selected {
                            TEXT_BRIGHT
                        } else {
                            TEXT
                        })
                    })
                    .collect();
                button(
                    row![
                        container(text(item.badge).size(11).font(MONO).color(MUTED))
                            .width(18)
                            .center_x(18),
                        rich_text(spans).size(13),
                        space().width(Fill),
                        text(item.detail.clone())
                            .size(12)
                            .color(MUTED)
                            .wrapping(iced::widget::text::Wrapping::None),
                    ]
                    .spacing(10)
                    .align_y(Center),
                )
                .on_press(Message::PalettePick(i))
                .width(Fill)
                .height(ROW)
                .padding([0, 10])
                .style(move |_: &Theme, status| button::Style {
                    background: match (selected, status) {
                        (true, _) => Some(SELECTED_STRONG.into()),
                        (false, button::Status::Hovered) => Some(SELECTED.into()),
                        _ => None,
                    },
                    text_color: TEXT,
                    border: iced::border::rounded(6),
                    ..Default::default()
                })
                .into()
            });
        let empty = p.items.is_empty() && (!p.query.is_empty() || matches!(p.mode, Mode::Recent));
        let list: Element<'_, Message> = if empty {
            container(text("Nothing found").size(13).color(MUTED))
                .padding([10, 14])
                .into()
        } else {
            column(rows).spacing(1).padding(6).into()
        };
        let footer = row![
            text(p.mode.title()).size(11).color(MUTED),
            space().width(Fill),
            text("↑↓ navigate   ↵ open   esc close")
                .size(11)
                .color(MUTED),
        ]
        .padding([7, 14]);
        let card = container(column![
            input,
            crate::ui::rule(),
            list,
            crate::ui::rule(),
            footer
        ])
        .width(Length::Fixed(680.0))
        .style(popup);
        container(card)
            .width(Fill)
            .height(Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .padding(Padding {
                top: 70.0,
                ..Padding::ZERO
            })
            .into()
    }
}
