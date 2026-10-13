//! Command handlers: editing commands, navigation popups, find/replace,
//! and find in path.

use crate::app::{App, Bottom, EDITOR_ID, Message, Search, TestTarget};
use crate::debug::DebugCommand;
use crate::editor::{Document, LineTransform, Surround};
use crate::keymap::Cmd;
use crate::palette::{self, Mode, Palette, Target};
use crate::search::{self, SearchDialog};
use crate::tr;
use iced::Task;
use iced::widget::Id;
use iced::widget::operation;

impl App {
    /// Dispatches a keymap command to its handler.
    ///
    /// Called from `update` for `Message::Command` and for shortcuts resolved by the
    /// keyboard handler. Commands that need an open document or project are no-ops
    /// without one; returns any focus/scroll task the handler produces.
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
            Cmd::QuickFix => self.open_intentions(),
            Cmd::ParameterInfo => {
                self.refresh_signature(true);
                Task::none()
            }
            Cmd::Rename => self.open_rename(),
            Cmd::OptimizeImports => self.optimize_imports(),
            Cmd::GotoImplementation => self.goto_implementation(),
            Cmd::TypeHierarchy => self.show_type_hierarchy(),
            Cmd::FileStructure if has_doc => self.open_palette(Mode::Structure),
            Cmd::FileStructure => Task::none(),
            Cmd::ToggleTerminal => self.toggle_terminal(),
            Cmd::ToggleClaude => self.toggle_claude(),
            Cmd::RunTests => self.run_tests(TestTarget::Context),
            Cmd::Reformat => self.reformat(),
            Cmd::Composer => self.open_composer(),
            Cmd::Generate => self.open_generate(),
            Cmd::InsertTemplate => self.open_templates(),
            Cmd::Commit => self.open_commit_panel(),
            Cmd::Push => self.git_push(),
            Cmd::Pull => self.git_pull(),
            Cmd::DebugTest => self.debug_test(),
            Cmd::ExtractVariable => self.extract_from_selection(false),
            Cmd::AddCaretAbove => self.add_caret_vertical(true),
            Cmd::AddCaretBelow => self.add_caret_vertical(false),
            Cmd::SelectNextOccurrence => self.select_next_occurrence(),
            Cmd::ToggleSoftWrap => {
                self.soft_wrap = !self.soft_wrap;
                self.reveal_cursor()
            }
            Cmd::ExtractMethod => self.extract_from_selection(true),
            Cmd::InlineVariable => self.inline_variable(),
            Cmd::ChangeSignature => self.change_signature_at_caret(),
            Cmd::MoveClass => self.start_refactor(crate::app::RefactorAction::MoveClass),
            Cmd::ToggleBreakpoint => self.toggle_breakpoint_at_caret(),
            Cmd::Resume => self.debug_send(DebugCommand::Resume),
            Cmd::StepOver => self.debug_send(DebugCommand::StepOver),
            Cmd::StepInto => self.debug_send(DebugCommand::StepInto),
            Cmd::StepOut => self.debug_send(DebugCommand::StepOut),
            Cmd::StopDebug => self.debug_send(DebugCommand::Stop),
            Cmd::Problems => {
                self.show_panel(Bottom::Problems);
                Task::none()
            }
            Cmd::ExtendSelection => self.caret_action(Document::extend_selection),
            Cmd::ShrinkSelection => self.caret_action(Document::shrink_selection),
            Cmd::MatchBrace => self.caret_action(Document::jump_to_matching_bracket),
            Cmd::BlockStart => self.caret_action(|d| d.jump_to_block_edge(false)),
            Cmd::BlockEnd => self.caret_action(|d| d.jump_to_block_edge(true)),
            Cmd::JoinLines => self.edit_with(Document::join_lines, None),
            Cmd::SplitLine => self.edit_with(Document::split_line, None),
            Cmd::StartNewLine => self.edit_with(|d| d.start_new_line(false), None),
            Cmd::StartNewLineBefore => self.edit_with(|d| d.start_new_line(true), None),
            Cmd::CompleteStatement => self.edit_with(Document::complete_statement, None),
            Cmd::ToggleCase => self.edit_with(Document::toggle_case, None),
            Cmd::Transpose => self.edit_with(Document::transpose, None),
            Cmd::BlockComment => self.edit_with(Document::toggle_block_comment, None),
            Cmd::WordCompletion => self.edit_with(Document::complete_word, None),
            Cmd::Unwrap => self.edit_with(Document::unwrap_block, None),
            Cmd::SortLines => self.edit_with(|d| d.transform_lines(LineTransform::Sort), None),
            Cmd::ReverseLines => {
                self.edit_with(|d| d.transform_lines(LineTransform::Reverse), None)
            }
            Cmd::UniqueLines => self.edit_with(|d| d.transform_lines(LineTransform::Unique), None),
            Cmd::SurroundWith if has_doc => self.open_surround(),
            Cmd::SurroundWith => Task::none(),
            Cmd::SelectAllOccurrences => self.select_all_occurrences(),
            Cmd::FindAction => self.open_find_action(),
            Cmd::NextError => self.goto_problem(true),
            Cmd::PreviousError => self.goto_problem(false),
            Cmd::QuickDoc => self.quick_doc(),
            Cmd::ToggleWhitespace => {
                self.show_whitespace = !self.show_whitespace;
                Task::none()
            }
            Cmd::MoveStatementUp => self.edit_with(|d| d.move_statement(false), None),
            Cmd::MoveStatementDown => self.edit_with(|d| d.move_statement(true), None),
            Cmd::MoveElementLeft => self.edit_with(|d| d.move_element(false), None),
            Cmd::MoveElementRight => self.edit_with(|d| d.move_element(true), None),
            Cmd::DeleteWordEnd => self.edit_with(|d| d.delete_word(true), None),
            Cmd::DeleteWordStart => self.edit_with(|d| d.delete_word(false), None),
            Cmd::AddCaretPerLine => self.caret_action(Document::add_caret_per_line),
            Cmd::ToggleBookmark => self.toggle_bookmark(),
            Cmd::ShowBookmarks => self.open_bookmarks(),
            Cmd::JumpToLastEdit => self.jump_to_last_edit(),
            Cmd::RecentLocations => self.open_recent_locations(),
            Cmd::PasteFromHistory if has_doc => self.open_clipboard_history(),
            Cmd::PasteFromHistory => Task::none(),
            Cmd::CopyReference => self.copy_reference(),
            Cmd::GotoSuperMethod => self.goto_super_method(),
            Cmd::GotoTypeDeclaration => self.goto_type_declaration(),
            Cmd::GotoTest => self.goto_test(),
            Cmd::ZoomIn => self.set_zoom(self.zoom + 0.1),
            Cmd::ZoomOut => self.set_zoom(self.zoom - 0.1),
            Cmd::ZoomReset => self.set_zoom(1.0),
            Cmd::CompareWithClipboard => self.compare_with_clipboard(),
            Cmd::LocalHistory => self.open_local_history(),
            Cmd::Annotate => self.toggle_annotate(),
            Cmd::QuickDefinition => self.quick_definition(),
            Cmd::ToggleDistractionFree => self.toggle_zen(),
            Cmd::IntroduceConstant => {
                self.introduce_from_selection(crate::app::RefactorAction::IntroduceConstant)
            }
            Cmd::IntroduceField => {
                self.introduce_from_selection(crate::app::RefactorAction::IntroduceField)
            }
            Cmd::IntroduceParameter => {
                self.introduce_from_selection(crate::app::RefactorAction::IntroduceParameter)
            }
        }
    }

    /// Sets the interface scale, kept between 50% and 300%.
    fn set_zoom(&mut self, zoom: f32) -> Task<Message> {
        self.zoom = (zoom * 10.0).round().clamp(5.0, 30.0) / 10.0;
        self.notice = Some(tr!(
            "notice-zoom",
            percent = format!("{:.0}", self.zoom * 100.0)
        ));
        Task::none()
    }

    /// Runs a caret-only action (no text change) on the active document and
    /// scrolls to the caret.
    fn caret_action(&mut self, action: impl FnOnce(&mut Document)) -> Task<Message> {
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        action(doc);
        self.reveal_cursor()
    }

    /// `⇧⌘A`: every command by name, with its shortcut, to run one.
    fn open_find_action(&mut self) -> Task<Message> {
        let items = Cmd::ALL
            .iter()
            .filter(|c| **c != Cmd::FindAction)
            .map(|&c| palette::Item {
                badge: "›",
                title: c.label(),
                matched: Vec::new(),
                detail: crate::keymap::hint(c).to_string(),
                target: Target::Action(c),
            })
            .collect();
        self.open_choice(tr!("action-find-action"), items)
    }

    /// `⌥⌘T`: the constructs the selection can be surrounded with.
    fn open_surround(&mut self) -> Task<Message> {
        let items = Surround::ALL
            .iter()
            .map(|&s| palette::Item {
                badge: "{",
                title: s.label().to_string(),
                matched: Vec::new(),
                detail: String::new(),
                target: Target::Surround(s),
            })
            .collect();
        self.open_choice(tr!("action-surround-with"), items)
    }

    /// `F2` / `⇧F2`: moves the caret to the next (or previous) problem in
    /// the file, wrapping around.
    fn goto_problem(&mut self, forward: bool) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let caret = doc.offset(doc.cursor());
        let starts: Vec<usize> = doc.problems().iter().map(|p| p.start).collect();
        let target = if forward {
            starts.iter().find(|s| **s > caret).or(starts.first())
        } else {
            starts.iter().rev().find(|s| **s < caret).or(starts.last())
        };
        let Some(&target) = target else {
            self.notice = Some(tr!("notice-no-problems"));
            return Task::none();
        };
        let position = doc.position_of(target);
        self.place_cursor(position)
    }

    // ----- palette ------------------------------------------------------------

    /// Opens the palette on a fixed list (implementations, Composer
    /// commands, the Generate menu) under `title`, and focuses its input.
    pub(crate) fn open_choice(
        &mut self,
        title: String,
        items: Vec<palette::Item>,
    ) -> Task<Message> {
        self.close_overlays();
        self.palette = Some(Palette {
            mode: Mode::Choose,
            query: String::new(),
            items: items.clone(),
            selected: 0,
            fixed: Some((title, items)),
            extra: None,
        });
        operation::focus(Id::new(palette::INPUT_ID))
    }

    /// Opens the navigation palette in `mode` with an empty query and focuses its input.
    ///
    /// Closes other overlays first. No-op without an open project. In Recent mode the
    /// second item is preselected so Enter switches to the previous file.
    pub(crate) fn open_palette(&mut self, mode: Mode) -> Task<Message> {
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
            fixed: None,
            extra: None,
        });
        operation::focus(Id::new(palette::INPUT_ID))
    }

    /// Refilters the open palette for a new query and resets the selection to the top.
    ///
    /// Called on every keystroke in the palette input.
    pub fn palette_query(&mut self, query: String) -> Task<Message> {
        let Some(mode) = self.palette.as_ref().map(|p| p.mode) else {
            return Task::none();
        };
        let mut items = match self.palette.as_ref().and_then(|p| p.fixed.as_ref()) {
            Some((_, fixed)) => filter_fixed(fixed, &query),
            None => self.palette_items(mode, &query),
        };
        let extra = self.palette.as_ref().and_then(|p| p.extra);
        if let Some(item) = extra.and_then(|build| build(&query))
            && !items.iter().any(|i| i.title == query)
        {
            items.push(item);
        }
        if let Some(p) = &mut self.palette {
            p.query = query;
            p.items = items;
            p.selected = 0;
        }
        Task::none()
    }

    /// Moves the palette selection by `delta`, wrapping at both ends.
    ///
    /// Called for the Up/Down arrow keys while the palette is open.
    pub fn palette_move(&mut self, delta: i32) -> Task<Message> {
        if let Some(p) = &mut self.palette {
            let n = p.items.len() as i32;
            if n > 0 {
                p.selected = ((p.selected as i32 + delta).rem_euclid(n)) as usize;
            }
        }
        Task::none()
    }

    /// Closes the palette and jumps to the selected item's file, symbol or line.
    ///
    /// Records the current location in navigation history first. With no selectable
    /// item it just returns focus to the editor.
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
            Target::Command(command) => self.run_in_terminal(&command),
            Target::Generate(generation) => self.generate(generation),
            Target::Template(abbreviation) => self.insert_template(abbreviation),
            Target::Checkout(branch) => self.git_checkout(branch),
            Target::CreateBranch(name) => self.git_create_branch(name),
            Target::FileAction(action) => self.run_file_action(action),
            Target::Action(cmd) => self.command(cmd),
            Target::LocalVersion(version) => self.open_local_version(version),
            Target::Location(path, line) => self.open_at_line(path, line),
            Target::Paste(text) => self.edit_with(
                |d| {
                    d.checkpoint();
                    let (a, b) = d
                        .selection()
                        .unwrap_or_else(|| (d.offset(d.cursor()), d.offset(d.cursor())));
                    d.replace(a, b, &text);
                },
                None,
            ),
            Target::Surround(construct) => self.edit_with(|d| d.surround_with(construct), None),
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

    /// Opens (or re-focuses) the find bar, optionally with the replace row.
    ///
    /// A single-line selection becomes the query. Matches are recomputed without
    /// jumping, and the query input is focused with its text selected.
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
            operation::focus(Id::new(search::FIND_INPUT)),
            operation::select_all(Id::new(search::FIND_INPUT)),
        ])
    }

    /// Updates the find query as the user types and jumps to the nearest match.
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

    /// Selects the match range in the active document and scrolls it into view.
    fn select_match(&mut self, (start, end): (usize, usize)) -> Task<Message> {
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        doc.select(start, end);
        self.reveal_cursor()
    }

    /// Moves to the next or previous find match and selects it.
    ///
    /// Backs Find Next / Find Previous; does nothing while the find bar is closed.
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

    /// Replaces the current match, then advances to the next one.
    ///
    /// If the current match is not what is selected yet, it only selects it, so the
    /// first press shows what will be replaced. The replacement is its own undo step.
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

    /// Replaces every match in the active document as one undo step.
    ///
    /// Shows a notice with the number of replacements; does nothing when there are none.
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
        self.notice = Some(tr!("notice-replaced", count = count));
        Task::none()
    }

    // ----- find in path -------------------------------------------------------

    /// Opens the find-in-path dialog, restoring the last search's settings.
    ///
    /// A single-line selection becomes the query. No-op without an open project.
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
            operation::focus(Id::new(search::SEARCH_INPUT)),
            operation::select_all(Id::new(search::SEARCH_INPUT)),
        ])
    }

    /// Starts a find-in-path search from the dialog's settings on a blocking thread.
    ///
    /// Closes the dialog (remembering its settings), shows the bottom Find panel in a
    /// running state, and returns focus to the editor. Searches files on disk, so
    /// unsaved edits are not seen. The result arrives as `Message::SearchDone`.
    /// An empty query keeps the dialog open and does nothing.
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
        self.show_panel(Bottom::Find);
        // Keep the dialog settings for next time, closed.
        self.last_search = Some(SearchDialog {
            error: None,
            ..dialog
        });
        Task::batch([
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        search::search_files(&files, &query, &options, &mask)
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::SearchDone,
            ),
            operation::focus(Id::new(EDITOR_ID)),
        ])
    }

    /// Replaces every match of the last find in path in the files it found:
    /// open tabs are edited (undoable, left unsaved), other files are
    /// written. The first press only asks for confirmation; afterwards the
    /// search runs again so the panel shows what is left.
    pub fn replace_in_path(&mut self) -> Task<Message> {
        let (Some(Search::Done(results, _)), Some(dialog)) =
            (&self.search, self.last_search.clone())
        else {
            return Task::none();
        };
        if !self.confirm_replace_in_path {
            self.confirm_replace_in_path = true;
            return Task::none();
        }
        self.confirm_replace_in_path = false;
        let paths: Vec<std::path::PathBuf> = results.files.iter().map(|f| f.path.clone()).collect();
        let (mut replaced, mut files, mut failed) = (0usize, 0usize, Vec::new());
        for path in paths {
            if let Some(doc) = self.docs.iter_mut().find(|d| d.path == path) {
                let Some((text, n)) =
                    dialog
                        .options
                        .replace_all(&dialog.query, &dialog.replacement, &doc.text)
                else {
                    continue;
                };
                if n > 0 {
                    let cursor = doc.cursor();
                    doc.set_text(text.clone(), cursor);
                    if doc.is_php() {
                        self.lang.update_file(&path, &text);
                    }
                    replaced += n;
                    files += 1;
                }
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                failed.push(path.display().to_string());
                continue;
            };
            let Some((new, n)) =
                dialog
                    .options
                    .replace_all(&dialog.query, &dialog.replacement, &text)
            else {
                continue;
            };
            if n == 0 {
                continue;
            }
            match std::fs::write(&path, &new) {
                Ok(()) => {
                    replaced += n;
                    files += 1;
                }
                Err(_) => failed.push(path.display().to_string()),
            }
        }
        self.inspect_pending = true;
        self.notice = Some(if failed.is_empty() {
            tr!("replace-in-path-done", count = replaced, files = files)
        } else {
            tr!("replace-in-path-failed", files = failed.join(", "))
        });
        self.search_dialog = Some(dialog);
        self.run_search()
    }

    /// Opens the file of hit `h` in result file `f` and selects the match.
    ///
    /// Marks the hit as selected in the panel and records navigation history. The
    /// selection is stored as pending so it applies once the file finishes loading.
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

    /// Scrolls the editor so `line` sits about a third of the way down the viewport.
    ///
    /// Also stores the offset on the document so it survives tab switches, and
    /// focuses the editor.
    fn place_selection_view(&mut self, line: usize) -> Task<Message> {
        let view = self.viewport_height;
        let row_top = self
            .doc()
            .map(|d| crate::app::row_top(&self.wrap_layout(d), line, 0))
            .unwrap_or_default();
        let y = (row_top - view / 3.0).max(0.0);
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

/// Filters a fixed list of palette items by fuzzy match on the title,
/// keeping the original order for an empty query.
pub(crate) fn filter_fixed(items: &[palette::Item], query: &str) -> Vec<palette::Item> {
    let mut scored: Vec<(i32, palette::Item)> = items
        .iter()
        .filter_map(|item| {
            let (score, matched) = palette::fuzzy(query, &item.title)?;
            Some((
                score,
                palette::Item {
                    matched,
                    ..item.clone()
                },
            ))
        })
        .collect();
    if !query.is_empty() {
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    }
    scored.into_iter().map(|(_, item)| item).collect()
}
