//! Command handlers: editing commands, navigation popups, find/replace,
//! and find in path.

use crate::app::{App, Bottom, EDITOR_ID, Message, Search};
use crate::find::{self, SearchDialog};
use crate::keymap::Cmd;
use crate::palette::{self, Mode, Palette, Target};
use iced::Task;
use iced::widget::Id;
use iced::widget::operation;

impl App {
    pub fn command(&mut self, cmd: Cmd) -> Task<Message> {
        let has_doc = self.doc().is_some();
        match cmd {
            Cmd::Save => self.save(),
            Cmd::CloseTab => match self.active {
                Some(i) => self.update(Message::CloseTab(i)),
                None => Task::none(),
            },
            Cmd::ToggleSidebar => self.update(Message::ToggleSidebar),
            Cmd::NextTab | Cmd::PrevTab => {
                let n = self.docs.len();
                match self.active {
                    Some(a) if n > 1 => {
                        let next = if cmd == Cmd::NextTab {
                            (a + 1) % n
                        } else {
                            (a + n - 1) % n
                        };
                        self.select(next)
                    }
                    _ => Task::none(),
                }
            }
            Cmd::Undo => self.edit_with(
                |d| {
                    d.undo();
                },
                None,
            ),
            Cmd::Redo => self.edit_with(
                |d| {
                    d.redo();
                },
                None,
            ),
            Cmd::ToggleComment => self.edit_with(|d| d.toggle_comment(), None),
            Cmd::Duplicate => self.edit_with(|d| d.duplicate(), None),
            Cmd::DeleteLine => self.edit_with(|d| d.delete_lines(), None),
            Cmd::MoveLineUp => self.edit_with(|d| d.move_lines(false), None),
            Cmd::MoveLineDown => self.edit_with(|d| d.move_lines(true), None),
            Cmd::Complete => {
                self.refresh_completion(true);
                Task::none()
            }
            Cmd::GotoDeclaration => self.goto_definition(),
            Cmd::FindUsages => self.find_usages(),
            Cmd::GotoLine if has_doc => self.open_palette(Mode::Line),
            Cmd::GotoLine => Task::none(),
            Cmd::GotoFile => self.open_palette(Mode::Files),
            Cmd::GotoClass => self.open_palette(Mode::Classes),
            Cmd::GotoSymbol => self.open_palette(Mode::Symbols),
            Cmd::RecentFiles => self.open_palette(Mode::Recent),
            Cmd::SearchEverywhere => self.open_palette(Mode::Everywhere),
            Cmd::Back => self.navigate(true),
            Cmd::Forward => self.navigate(false),
            Cmd::Find | Cmd::Replace if has_doc => self.open_find(cmd == Cmd::Replace),
            Cmd::Find | Cmd::Replace => Task::none(),
            Cmd::FindNext => self.find_step(true),
            Cmd::FindPrevious => self.find_step(false),
            Cmd::FindInPath => self.open_search_dialog(),
        }
    }

    // ----- palette ------------------------------------------------------------

    fn open_palette(&mut self, mode: Mode) -> Task<Message> {
        if self.tree.root.is_none() {
            return Task::none();
        }
        self.close_overlays();
        let query = String::new();
        let items = self.palette_items(mode, &query);
        // Recent files: preselect the previous file, like a quick switch.
        let selected = if mode == Mode::Recent && items.len() > 1 {
            1
        } else {
            0
        };
        self.palette = Some(Palette {
            mode,
            query,
            items,
            selected,
        });
        operation::focus(Id::new(palette::INPUT_ID))
    }

    pub fn palette_query(&mut self, query: String) -> Task<Message> {
        let Some(mode) = self.palette.as_ref().map(|p| p.mode) else {
            return Task::none();
        };
        let items = self.palette_items(mode, &query);
        if let Some(p) = &mut self.palette {
            p.query = query;
            p.items = items;
            p.selected = 0;
        }
        Task::none()
    }

    pub fn palette_move(&mut self, delta: i32) -> Task<Message> {
        if let Some(p) = &mut self.palette {
            let n = p.items.len() as i32;
            if n > 0 {
                p.selected = ((p.selected as i32 + delta).rem_euclid(n)) as usize;
            }
        }
        Task::none()
    }

    pub fn palette_open_selected(&mut self) -> Task<Message> {
        let Some(p) = self.palette.take() else {
            return Task::none();
        };
        let Some(item) = p.items.get(p.selected).cloned() else {
            return operation::focus(Id::new(EDITOR_ID));
        };
        self.record_nav();
        match item.target {
            Target::File(path) => self.open(path, None),
            Target::Symbol(path, pos) => self.open(path, Some(pos)),
            Target::Line(line, col) => {
                let Some(doc) = self.doc() else {
                    return Task::none();
                };
                let position = doc.position_at_visual(line, col);
                self.place_cursor(position)
            }
        }
    }

    // ----- find in file -------------------------------------------------------

    fn open_find(&mut self, replace: bool) -> Task<Message> {
        let selected = self
            .doc()
            .and_then(|d| d.selected_text())
            .filter(|s| !s.contains('\n'))
            .map(str::to_string);
        let mut bar = self.find.take().unwrap_or_default();
        if let Some(sel) = selected {
            bar.query = sel;
        }
        bar.show_replace = bar.show_replace || replace;
        self.find = Some(bar);
        let refresh = self.refresh_find(false);
        Task::batch([
            refresh,
            operation::focus(Id::new(find::FIND_INPUT)),
            operation::select_all(Id::new(find::FIND_INPUT)),
        ])
    }

    pub fn find_query(&mut self, query: String) -> Task<Message> {
        if let Some(f) = &mut self.find {
            f.query = query;
        }
        self.refresh_find(true)
    }

    /// Recomputes matches; with `jump`, selects the match nearest the cursor.
    pub fn refresh_find(&mut self, jump: bool) -> Task<Message> {
        let (Some(find), Some(doc)) = (
            self.find.as_mut(),
            self.active.and_then(|i| self.docs.get(i)),
        ) else {
            return Task::none();
        };
        let from = doc
            .selection()
            .map(|(a, _)| a)
            .unwrap_or_else(|| doc.offset(doc.cursor()));
        find.recompute(&doc.text, from);
        if !jump {
            return Task::none();
        }
        let Some(m) = find.current.map(|i| find.matches[i]) else {
            return Task::none();
        };
        self.select_match(m)
    }

    fn select_match(&mut self, (start, end): (usize, usize)) -> Task<Message> {
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        doc.select(start, end);
        self.reveal_cursor()
    }

    pub fn find_step(&mut self, forward: bool) -> Task<Message> {
        let Some(find) = &mut self.find else {
            // ⌘G without the bar: reuse the last query if there was one.
            return Task::none();
        };
        match find.step(forward) {
            Some(m) => self.select_match(m),
            None => Task::none(),
        }
    }

    pub fn replace_one(&mut self) -> Task<Message> {
        let (Some(find), Some(i)) = (self.find.as_ref(), self.active) else {
            return Task::none();
        };
        let doc = &self.docs[i];
        let Some(m) = find.current.map(|c| find.matches[c]) else {
            return Task::none();
        };
        // Only replace when the current match is what is selected.
        if doc.selection() != Some(m) {
            return self.select_match(m);
        }
        let Some(with) = find.replacement_for(&doc.text, m) else {
            return Task::none();
        };
        let doc = &mut self.docs[i];
        doc.checkpoint();
        doc.replace(m.0, m.1, &with);
        self.after_change(true);
        self.refresh_find(true)
    }

    pub fn replace_all(&mut self) -> Task<Message> {
        let (Some(find), Some(i)) = (self.find.as_ref(), self.active) else {
            return Task::none();
        };
        let Some((text, count)) = find.replace_all(&self.docs[i].text) else {
            return Task::none();
        };
        if count == 0 {
            return Task::none();
        }
        let cursor = self.docs[i].cursor();
        self.docs[i].set_text(text, cursor);
        self.after_change(true);
        self.notice = Some(format!(
            "Replaced {count} occurrence{}",
            if count == 1 { "" } else { "s" }
        ));
        Task::none()
    }

    // ----- find in path -------------------------------------------------------

    fn open_search_dialog(&mut self) -> Task<Message> {
        if self.tree.root.is_none() {
            return Task::none();
        }
        let selected = self
            .doc()
            .and_then(|d| d.selected_text())
            .filter(|s| !s.contains('\n'))
            .map(str::to_string);
        let mut dialog = self
            .search_dialog
            .take()
            .or_else(|| self.last_search.clone())
            .unwrap_or_default();
        if let Some(sel) = selected {
            dialog.query = sel;
        }
        self.search_dialog = Some(dialog);
        Task::batch([
            operation::focus(Id::new(find::SEARCH_INPUT)),
            operation::select_all(Id::new(find::SEARCH_INPUT)),
        ])
    }

    pub fn run_search(&mut self) -> Task<Message> {
        let Some(dialog) = self.search_dialog.take() else {
            return Task::none();
        };
        if dialog.query.is_empty() {
            self.search_dialog = Some(dialog);
            return Task::none();
        }
        let mut files = self.project_files.clone();
        if dialog.include_vendor {
            files.extend(self.vendor_files.iter().cloned());
        }
        // Unsaved buffers are searched as saved: save first for exact results.
        let (query, options, mask) = (
            dialog.query.clone(),
            dialog.options.clone(),
            dialog.mask.clone(),
        );
        self.search = Some(Search::Running(query.clone()));
        self.bottom = Some(Bottom::Find);
        // Keep the dialog settings for next time, closed.
        self.last_search = Some(SearchDialog {
            error: None,
            ..dialog
        });
        Task::batch([
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        find::search_files(&files, &query, &options, &mask)
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::SearchDone,
            ),
            operation::focus(Id::new(EDITOR_ID)),
        ])
    }

    pub fn open_search_hit(&mut self, f: usize, h: usize) -> Task<Message> {
        let Some(Search::Done(results, selected)) = &mut self.search else {
            return Task::none();
        };
        let Some(file) = results.files.get(f) else {
            return Task::none();
        };
        let Some(hit) = file.hits.get(h) else {
            return Task::none();
        };
        *selected = Some((f, h));
        let path = file.path.clone();
        let (line, start, end) = (hit.line, hit.start, hit.end);
        self.record_nav();
        // The file may still be loading: the selection waits for it.
        self.pending_select = Some((path.clone(), line, start, end));
        let open = self.open(path, None);
        Task::batch([open, self.apply_pending_select()])
    }

    fn place_selection_view(&mut self, line: usize) -> Task<Message> {
        let view = self.viewport_height;
        let y = (crate::theme::EDITOR_PAD_TOP + line as f32 * crate::theme::CODE_LINE - view / 3.0)
            .max(0.0);
        if let Some(doc) = self.doc_mut() {
            doc.scroll_y = y;
        }
        Task::batch([
            crate::app::scroll_to(y),
            operation::focus(Id::new(EDITOR_ID)),
        ])
    }

    /// Applies a selection that waited for its file to load.
    pub fn apply_pending_select(&mut self) -> Task<Message> {
        let Some((path, line, start, end)) = self.pending_select.take() else {
            return Task::none();
        };
        let Some(doc) = self.doc_mut().filter(|d| d.path == path) else {
            self.pending_select = Some((path, line, start, end));
            return Task::none();
        };
        let base = doc.line_start(line);
        doc.select(base + start, base + end);
        self.place_selection_view(line)
    }
}
