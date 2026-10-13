//! Code completion in the editor: when the popup opens by itself, ranking
//! of the engine's proposals (match quality, how often each was picked
//! before, proximity, length), keyword proposals, and inserting a choice
//! (parentheses with the caret inside, `use` imports, Tab replacing the
//! rest of the word).

use super::*;
use crate::editor::Token;
use crate::tr;
use iced::Task;
use iced::widget::text_editor::Position;
use phpthunder::analysis::{Candidate, KEYWORD_DETAIL, keyword_candidates, rank_candidates};
use std::collections::HashMap;
use std::path::PathBuf;

/// Most proposals kept in the popup.
const MAX_ITEMS: usize = 200;

/// File keeping how often each proposal was picked, across sessions.
fn stats_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/phpthunder/completion-stats.json"))
}

/// Key of a proposal in the usage statistics: its label and detail (FQN
/// or signature), so `save` of two classes count apart.
fn stats_key(candidate: &Candidate) -> String {
    format!("{}\u{1}{}", candidate.label, candidate.detail)
}

/// Reads saved usage statistics; empty when missing or unreadable.
pub(crate) fn load_completion_stats() -> HashMap<String, u32> {
    stats_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Whether a signature (`function save(array $options = [])`) declares
/// parameters, so completion leaves the caret inside the parentheses.
fn has_parameters(signature: &str) -> bool {
    signature
        .split_once('(')
        .is_some_and(|(_, rest)| !rest.trim_start().starts_with(')'))
}

impl App {
    /// Whether typing `c` should open completion unprompted: after `->`,
    /// `::`, `$`, or the first character of a new identifier in PHP code,
    /// and never inside comments or strings (Laravel key strings aside,
    /// handled by the caller).
    pub(crate) fn should_auto_complete(&self, c: char) -> bool {
        let Some(doc) = self.doc() else { return false };
        if doc.file_type == crate::filetype::FileType::Blade {
            return self
                .blade_candidates(&doc.text, doc.offset(doc.cursor()))
                .is_some();
        }
        if !doc.is_php() {
            return false;
        }
        let offset = doc.offset(doc.cursor());
        let before = &doc.text[..offset];
        let start = before.len().saturating_sub(c.len_utf8());
        if matches!(
            doc.token_at(start),
            Some(Token::Comment | Token::String | Token::DocTag)
        ) {
            return false;
        }
        match c {
            '>' => before.ends_with("->"),
            ':' => before.ends_with("::"),
            '$' => true,
            c if is_ident(c) => {
                // Start of a new word only, so the popup does not flicker per key.
                let word = phpthunder::analysis::ident_start(&doc.text, offset);
                offset - word == c.len_utf8()
            }
            _ => false,
        }
    }

    /// Recomputes completion proposals at the cursor and opens or updates
    /// the popup, ranked best first. The selection follows the previously
    /// selected proposal when it is still listed, else the top one.
    /// `explicit` is set for `Cmd::Complete`, which shows a "No
    /// suggestions" notice when empty; implicit refreshes from typing just
    /// close the popup.
    pub fn refresh_completion(&mut self, explicit: bool) {
        let Some(doc) = self.doc() else { return };
        let offset = doc.offset(doc.cursor());
        let keys = self
            .laravel_candidates(&doc.text, offset)
            .or_else(|| self.blade_candidates(&doc.text, offset));
        let (start, items) = match keys {
            Some(keys) => keys,
            None if doc.file_type == crate::filetype::FileType::Blade => (offset, Vec::new()),
            None => {
                let mut start = phpthunder::analysis::ident_start(&doc.text, offset);
                if start > 0 && doc.text.as_bytes()[start - 1] == b'$' {
                    start -= 1;
                }
                let mut items = self.lang.complete(&doc.text, offset);
                if doc.is_php() && !doc.text[start..offset].starts_with('$') && offset > start {
                    items.extend(keyword_candidates(&doc.text, offset));
                }
                (start, items)
            }
        };
        let prefix = doc.text[start..offset].to_string();
        let stats = &self.completion_stats;
        let ranked = rank_candidates(
            &prefix,
            items,
            |c| stats.get(&stats_key(c)).copied().unwrap_or(0),
            MAX_ITEMS,
        );
        if ranked.is_empty() {
            self.completion = None;
            if explicit {
                self.notice = Some(tr!("notice-no-suggestions"));
            }
            return;
        }
        let pos = doc.position_of(start);
        let (line, col) = (pos.line, doc.visual_column(pos));
        let previous = self
            .completion
            .as_ref()
            .and_then(|c| c.items.get(c.selected))
            .map(|r| stats_key(&r.candidate));
        let selected = previous
            .and_then(|key| ranked.iter().position(|r| stats_key(&r.candidate) == key))
            .unwrap_or(0);
        self.completion = Some(Completion {
            items: ranked,
            selected,
            start,
            line,
            col,
        });
    }

    /// Inserts the selected proposal as one undo step. Enter inserts at the
    /// caret; with `replace` (Tab) the rest of the word after the caret is
    /// replaced too. Calls get `()` with the caret inside when they take
    /// parameters (and parameter info opens), after them otherwise; an
    /// existing `(` is reused. A `use` import is added when the proposal
    /// needs one. The choice is counted in the usage statistics.
    pub(crate) fn accept_completion(&mut self, replace: bool) -> Task<Message> {
        let Some(completion) = self.completion.take() else {
            return Task::none();
        };
        let Some(item) = completion
            .items
            .get(completion.selected)
            .map(|r| r.candidate.clone())
        else {
            return Task::none();
        };
        *self.completion_stats.entry(stats_key(&item)).or_default() += 1;
        self.save_completion_stats();
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let mut start = completion.start.min(offset);
        // A variable proposal already carries `$`; a member never does.
        if !item.insert_text.starts_with('$') && doc.text[start..offset].starts_with('$') {
            start += 1;
        }
        let mut end = offset;
        if replace {
            end += doc.text[offset..]
                .find(|c: char| !is_ident(c))
                .unwrap_or(doc.text.len() - offset);
        }
        let mut text = item.insert_text.clone();
        let mut caret_back = 0;
        let mut show_params = false;
        if let Some(name) = text.strip_suffix('(') {
            let next = doc.text[end..].chars().next();
            if next == Some('(') {
                // `foo|()`: reuse the existing parentheses.
                text = name.to_string();
            } else {
                // Keywords and Blade directives with `(` take arguments.
                let inside = item.detail == KEYWORD_DETAIL
                    || item.detail == "directive"
                    || has_parameters(&item.detail);
                text = format!("{name}()");
                if inside {
                    caret_back = 1;
                    show_params = item.detail != KEYWORD_DETAIL && item.detail != "directive";
                }
            }
        }
        doc.checkpoint();
        doc.replace(start, end, &text);
        if caret_back > 0 {
            let at = doc.offset(doc.cursor()) - caret_back;
            let position = doc.position_of(at);
            doc.move_to(position);
        }
        let after = doc.cursor();
        if let Some((at, import)) = &item.import
            && *at <= start
        {
            doc.replace(*at, *at, import);
            let shift = import.matches('\n').count();
            doc.move_to(Position {
                line: after.line + shift,
                column: after.column,
            });
        }
        self.after_change(true);
        if show_params {
            self.refresh_signature(true);
        }
        self.reveal_cursor()
    }

    /// Blade completion in the active Blade template: component tags and
    /// attributes, directives, and view variables. `None` outside those
    /// contexts or outside a Laravel project.
    pub(crate) fn blade_candidates(
        &self,
        text: &str,
        offset: usize,
    ) -> Option<(usize, Vec<Candidate>)> {
        let doc = self
            .doc()
            .filter(|d| d.file_type == crate::filetype::FileType::Blade)?;
        let project = self.laravel.as_ref()?;
        let views = self.tree.root.as_ref()?.join("resources/views");
        let view = doc.path.strip_prefix(&views).ok().map(|rel| {
            rel.to_string_lossy()
                .trim_end_matches(".blade.php")
                .replace(['/', '\\'], ".")
        });
        phpthunder::laravel::blade_completions(project, text, offset, view.as_deref())
    }

    /// Writes the usage statistics; failures only cost ranking memory.
    fn save_completion_stats(&self) {
        let Some(path) = stats_path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string(&self.completion_stats) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_parameters_should_read_the_signature() {
        assert!(has_parameters(
            "public function save(array $options = []): bool"
        ));
        assert!(!has_parameters("public function fresh(): static"));
        assert!(!has_parameters("App\\Models\\User"));
    }
}
