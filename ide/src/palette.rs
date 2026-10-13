//! Navigation popup: go to file / class / symbol / line, recent files and
//! "search everywhere", with fuzzy matching.

use crate::app::{App, Message};
use crate::theme::{self, *};
use iced::widget::text::Span;
use iced::widget::{Id, button, column, container, rich_text, row, space, text, text_input};
use iced::{Center, Element, Fill, Font, Length, Padding, Theme};
use phpthunder::stubs::{Kind, Symbol};
use phpthunder::text::Pos;
use std::path::{Path, PathBuf};

/// Widget id of the query input, focused by `open_palette` in actions.rs.
pub const INPUT_ID: &str = "palette-input";
/// Cap on ranked results kept per query.
const MAX_ITEMS: usize = 60;
/// Rows shown at once; the list window follows the selection.
const VISIBLE_ROWS: usize = 12;
/// Height in px of one result row.
const ROW: f32 = 30.0;

/// What the palette searches, chosen by the shortcut that opened it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Project files, matched by name then by relative path.
    Files,
    /// Class-like symbols: classes, interfaces, traits and enums.
    Classes,
    /// Every non-virtual indexed symbol, members included.
    Symbols,
    /// Recently opened files, most recent first.
    Recent,
    /// A `line[:column]` in the active document.
    Line,
    /// Files and symbols together, files slightly demoted.
    Everywhere,
}

impl Mode {
    /// Mode name shown in the palette footer.
    fn title(self) -> &'static str {
        match self {
            Mode::Files => "Go to file",
            Mode::Classes => "Go to class",
            Mode::Symbols => "Go to symbol",
            Mode::Recent => "Recent files",
            Mode::Line => "Go to line",
            Mode::Everywhere => "Search everywhere",
        }
    }

    /// Placeholder text for the query input.
    fn placeholder(self) -> &'static str {
        match self {
            Mode::Line => "line[:column]",
            Mode::Recent => "Filter recent files",
            _ => "Type a name",
        }
    }
}

/// Where picking an item navigates; handled by `palette_open_selected` in actions.rs.
#[derive(Debug, Clone)]
pub enum Target {
    /// Open a file.
    File(PathBuf),
    /// Open a file with the caret at a symbol's name.
    Symbol(PathBuf, Pos),
    /// Move the caret in the active document to `(line, visual column)`, both 0-based.
    Line(usize, usize),
}

/// One ranked palette result.
#[derive(Debug, Clone)]
pub struct Item {
    /// Kind marker in the left column (`▤` file, `#` line, or a symbol letter).
    pub badge: &'static str,
    /// Main label; matched characters are drawn in the accent color.
    pub title: String,
    /// Character indices of `title` that matched the query.
    pub matched: Vec<usize>,
    /// Right-aligned context: relative path, or namespace and `file:line`.
    pub detail: String,
    /// Destination when the item is picked.
    pub target: Target,
}

/// State of the open navigation palette, held in `App::palette` while visible.
pub struct Palette {
    /// What is being searched.
    pub mode: Mode,
    /// Current input text.
    pub query: String,
    /// Ranked results for `query`, recomputed on every keystroke.
    pub items: Vec<Item>,
    /// Index into `items` of the highlighted row.
    pub selected: usize,
}

/// Fuzzy subsequence match. Returns a score (higher is better) and the
/// matched character indices, or `None` when not all query chars appear.
pub fn fuzzy(query: &str, candidate: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let chars: Vec<char> = candidate.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
    let q: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let mut matched = Vec::with_capacity(q.len());
    let mut score = 0i32;
    let mut qi = 0;
    let mut last: Option<usize> = None;
    for (i, &c) in lower.iter().enumerate() {
        if qi == q.len() {
            break;
        }
        if c != q[qi] {
            continue;
        }
        let boundary = i == 0
            || matches!(chars[i - 1], '\\' | '/' | '_' | '-' | '.' | ' ' | ':' | '$')
            || (chars[i].is_uppercase() && chars[i - 1].is_lowercase());
        score += 1;
        if boundary {
            score += 12;
        }
        match last {
            Some(l) if l + 1 == i => score += 8,
            Some(l) => score -= ((i - l - 1) as i32).min(6),
            None => score -= (i as i32).min(8),
        }
        matched.push(i);
        last = Some(i);
        qi += 1;
    }
    if qi < q.len() {
        return None;
    }
    // Prefer shorter candidates and exact case.
    score -= (chars.len() as i32 / 8).min(10);
    if candidate.starts_with(query) {
        score += 20;
    }
    Some((score, matched))
}

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

/// Kind letter for symbol items; matches the completion badges in view.rs.
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
    /// `MAX_ITEMS`. Called from actions.rs when the palette opens and on each
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
            .style(theme::input);
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
            crate::view::rule(),
            list,
            crate::view::rule(),
            footer
        ])
        .width(Length::Fixed(680.0))
        .style(theme::popup);
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

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn fuzzy_prefers_boundaries_and_prefixes() {
        let (a, _) = fuzzy("uc", "UserController.php").unwrap();
        let (b, _) = fuzzy("uc", "Structure.php").unwrap();
        assert!(a > b);
        assert!(fuzzy("xyz", "User.php").is_none());
        let (_, m) = fuzzy("pc", "PostController").unwrap();
        assert_eq!(m, vec![0, 4]);
        let (exact, _) = fuzzy("Post", "Post").unwrap();
        let (longer, _) = fuzzy("Post", "PostController").unwrap();
        assert!(exact > longer);
    }
}
