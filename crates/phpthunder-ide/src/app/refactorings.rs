//! Refactorings beyond rename: extract variable and method, inline
//! variable (in the active file), change signature and move class (across
//! the project), from the `⌥↵` menu or their shortcuts.

use super::*;
use crate::tr;
use iced::Task;
use iced::widget::{Id, operation};
use phpthunder::refactor::{self, LocalRefactoring, ParamChange, RenamePlan};
use phpthunder::syntax::parse;
use std::path::{Path, PathBuf};

/// Widget id of the refactoring prompt's input.
pub const REFACTOR_INPUT: &str = "refactor-input";

/// The parameter list of the function or method declared or called at
/// `offset`, as written in its declaration (`int $a, string $b = ''`).
fn parameters_text(lang: &crate::language::Language, text: &str, offset: usize) -> Option<String> {
    let def = lang.definitions(text, offset).into_iter().next()?;
    let signature = def.symbol.signature;
    let open = signature.find('(')?;
    let close = signature.rfind(')')?;
    Some(signature[open + 1..close].trim().to_string())
}

/// `$name` of a parameter declaration (`?int $count = 0` -> `$count`).
fn parameter_variable(declaration: &str) -> Option<&str> {
    declaration
        .split_whitespace()
        .map(|w| w.trim_start_matches(['&', '.']))
        .find(|w| w.starts_with('$'))
        .map(|w| w.trim_end_matches(','))
}

/// Splits a parameter list on top-level commas.
fn split_parameters(list: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in list.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '(' | '[') => depth += 1,
            (None, ')' | ']') => depth -= 1,
            (None, ',') if depth == 0 => {
                out.push(std::mem::take(&mut current).trim().to_string());
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

/// Parameter changes from the old and new parameter lists: parameters are
/// matched by variable name; a new one passes its default value (or
/// `null`) at existing calls.
pub(crate) fn parameter_changes(old: &str, new: &str) -> Vec<ParamChange> {
    let old_names: Vec<String> = split_parameters(old)
        .iter()
        .filter_map(|p| parameter_variable(p).map(str::to_string))
        .collect();
    split_parameters(new)
        .into_iter()
        .map(|declaration| {
            let name = parameter_variable(&declaration).map(str::to_string);
            let from = name
                .as_ref()
                .and_then(|n| old_names.iter().position(|o| o == n));
            let default_argument = declaration
                .split_once('=')
                .map(|(_, v)| v.trim().to_string())
                .unwrap_or_else(|| "null".into());
            ParamChange {
                from,
                declaration,
                default_argument,
            }
        })
        .collect()
}

/// The namespace of the class in `src`, if declared.
fn namespace_of(src: &str) -> String {
    let tree = parse(src);
    phpthunder::syntax::context_at(tree.root_node(), src, src.len()).namespace
}

/// The folder a class in `namespace` belongs in, from the PSR-4 maps of
/// `composer.json` (the longest matching prefix wins); `None` when no
/// mapping covers it.
pub(crate) fn folder_for_namespace(root: &Path, namespace: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(root.join("composer.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let mut best: Option<(usize, PathBuf)> = None;
    for section in ["autoload", "autoload-dev"] {
        let Some(map) = json
            .get(section)
            .and_then(|a| a.get("psr-4"))
            .and_then(|m| m.as_object())
        else {
            continue;
        };
        for (prefix, folders) in map {
            let prefix = prefix.trim_end_matches('\\');
            let rest = if namespace == prefix {
                ""
            } else if let Some(rest) = namespace.strip_prefix(&format!("{prefix}\\")) {
                rest
            } else {
                continue;
            };
            let folder = match folders {
                serde_json::Value::String(s) => s.as_str(),
                serde_json::Value::Array(a) => {
                    a.first().and_then(|v| v.as_str()).unwrap_or_default()
                }
                _ => continue,
            };
            if best.as_ref().is_some_and(|(len, _)| *len > prefix.len()) {
                continue;
            }
            let mut dir = root.join(folder.trim_end_matches('/'));
            for part in rest.split('\\').filter(|p| !p.is_empty()) {
                dir.push(part);
            }
            best = Some((prefix.len(), dir));
        }
    }
    best.map(|(_, d)| d)
}

/// A name for a value from its expression: the last called or accessed
/// name (`$order->getTotal()` → `total`), else `value`.
fn suggested_identifier(expression: &str) -> String {
    expression
        .rsplit(['>', ':'])
        .next()
        .and_then(|c| c.split('(').next())
        .map(|c| c.trim().trim_start_matches("get").trim_start_matches('$'))
        .filter(|c| !c.is_empty() && c.chars().all(|ch| ch.is_alphanumeric() || ch == '_'))
        .map(|c| {
            let mut chars = c.chars();
            chars
                .next()
                .map(|f| f.to_lowercase().chain(chars).collect())
                .unwrap_or_default()
        })
        .unwrap_or_else(|| "value".to_string())
}

impl App {
    /// Refactorings that apply at the caret or selection, for the `⌥↵`
    /// menu.
    pub(crate) fn refactor_intentions(&self) -> Vec<Intention> {
        let Some(doc) = self.doc().filter(|d| d.is_php()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some((start, end)) = doc.selection().filter(|(s, e)| s < e) {
            out.push(Intention::Refactor(RefactorAction::ExtractVariable(
                start, end,
            )));
            out.push(Intention::Refactor(RefactorAction::ExtractMethod(
                start, end,
            )));
            out.push(Intention::Refactor(RefactorAction::IntroduceConstant(
                start, end,
            )));
            out.push(Intention::Refactor(RefactorAction::IntroduceField(
                start, end,
            )));
            out.push(Intention::Refactor(RefactorAction::IntroduceParameter(
                start, end,
            )));
        }
        let offset = doc.offset(doc.cursor());
        let on_variable = doc.text[..offset]
            .ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '$')
            && phpthunder::analysis::ident_start(&doc.text, offset) > 0
            && doc.text.as_bytes()[phpthunder::analysis::ident_start(&doc.text, offset) - 1]
                == b'$';
        if on_variable {
            out.push(Intention::InlineVariable);
        }
        let callable = self
            .lang
            .definitions(&doc.text, offset)
            .first()
            .is_some_and(|d| {
                matches!(
                    d.symbol.kind,
                    phpthunder::symbol::Kind::Method | phpthunder::symbol::Kind::Function
                )
            });
        if callable {
            out.push(Intention::Refactor(RefactorAction::ChangeSignature(offset)));
        }
        if refactor::class_outline(&doc.text, offset).is_some() {
            out.push(Intention::Refactor(RefactorAction::MoveClass));
        }
        out
    }

    /// Introduce constant (`⌥⌘C`), field (`⌥⌘F`) or parameter (`⌥⌘P`)
    /// from the selection.
    pub(crate) fn introduce_from_selection(
        &mut self,
        make: fn(usize, usize) -> RefactorAction,
    ) -> Task<Message> {
        let Some((start, end)) = self
            .doc()
            .and_then(|d| d.selection())
            .filter(|(s, e)| s < e)
        else {
            self.notice = Some(tr!("refactor-select-first"));
            return Task::none();
        };
        self.start_refactor(make(start, end))
    }

    /// Extract variable (`⌥⌘V`) or method (`⌥⌘M`) from the selection.
    pub(crate) fn extract_from_selection(&mut self, method: bool) -> Task<Message> {
        let Some((start, end)) = self
            .doc()
            .and_then(|d| d.selection())
            .filter(|(s, e)| s < e)
        else {
            self.notice = Some(tr!("refactor-select-first"));
            return Task::none();
        };
        let action = if method {
            RefactorAction::ExtractMethod(start, end)
        } else {
            RefactorAction::ExtractVariable(start, end)
        };
        self.start_refactor(action)
    }

    /// Change signature (`⌘F6`) of the function or method at the caret.
    pub(crate) fn change_signature_at_caret(&mut self) -> Task<Message> {
        let Some(offset) = self.doc().map(|d| d.offset(d.cursor())) else {
            return Task::none();
        };
        self.start_refactor(RefactorAction::ChangeSignature(offset))
    }

    /// Opens the prompt of a refactoring with a sensible starting value.
    pub(crate) fn start_refactor(&mut self, action: RefactorAction) -> Task<Message> {
        let Some(doc) = self.doc().filter(|d| d.is_php()) else {
            return Task::none();
        };
        let (title, help, value) = match &action {
            RefactorAction::ExtractVariable(start, end) => {
                let expression = &doc.text[*start..*end];
                let suggested = expression
                    .rsplit(['>', ':'])
                    .next()
                    .and_then(|c| c.split('(').next())
                    .map(|c| c.trim().trim_start_matches("get").trim_start_matches('$'))
                    .filter(|c| {
                        !c.is_empty() && c.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
                    })
                    .map(|c| {
                        let mut chars = c.chars();
                        chars
                            .next()
                            .map(|f| f.to_lowercase().chain(chars).collect())
                            .unwrap_or_default()
                    })
                    .unwrap_or_else(|| "value".to_string());
                (
                    tr!("refactor-extract-variable"),
                    tr!("refactor-extract-variable-help"),
                    format!("${suggested}"),
                )
            }
            RefactorAction::ExtractMethod(..) => (
                tr!("refactor-extract-method"),
                tr!("refactor-extract-method-help"),
                String::new(),
            ),
            RefactorAction::IntroduceConstant(start, end) => (
                tr!("refactor-introduce-constant"),
                tr!("refactor-introduce-constant-help"),
                suggested_identifier(&doc.text[*start..*end]).to_uppercase(),
            ),
            RefactorAction::IntroduceField(start, end) => (
                tr!("refactor-introduce-field"),
                tr!("refactor-introduce-field-help"),
                suggested_identifier(&doc.text[*start..*end]),
            ),
            RefactorAction::IntroduceParameter(start, end) => (
                tr!("refactor-introduce-parameter"),
                tr!("refactor-introduce-parameter-help"),
                format!("${}", suggested_identifier(&doc.text[*start..*end])),
            ),
            RefactorAction::ChangeSignature(offset) => {
                match parameters_text(&self.lang, &doc.text, *offset) {
                    Some(params) => (
                        tr!("refactor-change-signature"),
                        tr!("refactor-change-signature-help"),
                        params,
                    ),
                    None => {
                        self.notice = Some(tr!("refactor-no-callable"));
                        return Task::none();
                    }
                }
            }
            RefactorAction::MoveClass => (
                tr!("refactor-move-class"),
                tr!("refactor-move-class-help"),
                namespace_of(&doc.text),
            ),
        };
        self.close_overlays();
        self.refactor_prompt = Some(RefactorPrompt {
            action,
            title,
            help,
            value,
            error: None,
        });
        Task::batch([
            operation::focus(Id::new(REFACTOR_INPUT)),
            operation::select_all(Id::new(REFACTOR_INPUT)),
        ])
    }

    /// Runs the prompted refactoring with the typed value.
    pub(crate) fn submit_refactor(&mut self) -> Task<Message> {
        let (Some(prompt), Some(doc)) = (self.refactor_prompt.as_ref(), self.doc()) else {
            return Task::none();
        };
        let value = prompt.value.trim().to_string();
        let (path, text) = (doc.path.clone(), doc.text.clone());
        let local = match prompt.action.clone() {
            RefactorAction::ExtractVariable(start, end) => {
                refactor::extract_variable(&text, start, end, Some(&value))
            }
            RefactorAction::ExtractMethod(start, end) => {
                refactor::extract_method(&text, start, end, &value)
            }
            RefactorAction::IntroduceConstant(start, end) => {
                refactor::introduce_constant(&text, start, end, Some(&value))
            }
            RefactorAction::IntroduceField(start, end) => {
                refactor::introduce_field(&text, start, end, Some(&value))
            }
            RefactorAction::IntroduceParameter(start, end) => {
                refactor::introduce_parameter(&text, start, end, Some(&value))
            }
            RefactorAction::ChangeSignature(offset) => {
                let old = parameters_text(&self.lang, &text, offset).unwrap_or_default();
                let params = parameter_changes(&old, &value);
                let open = self.open_texts();
                return Task::perform(
                    self.lang
                        .clone()
                        .change_signature(text, offset, params, open),
                    Message::RefactorPlanned,
                );
            }
            RefactorAction::MoveClass => {
                let namespace = value.trim_matches('\\').to_string();
                let new_path = self.tree.root.as_deref().and_then(|root| {
                    let folder = folder_for_namespace(root, &namespace)?;
                    Some(folder.join(path.file_name()?))
                });
                let open = self.open_texts();
                return Task::perform(
                    self.lang
                        .clone()
                        .move_class(path, text, namespace, new_path, open),
                    Message::RefactorPlanned,
                );
            }
        };
        match local {
            Ok(plan) => {
                self.refactor_prompt = None;
                self.apply_local(plan)
            }
            Err(error) => {
                if let Some(p) = &mut self.refactor_prompt {
                    p.error = Some(error);
                }
                Task::none()
            }
        }
    }

    /// Applies a single-file refactoring and moves the caret where it says.
    fn apply_local(&mut self, plan: LocalRefactoring) -> Task<Message> {
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        doc.apply_edits(&plan.edits);
        let position = doc.position_of(plan.caret.min(doc.text.len()));
        doc.move_to(position);
        self.after_change(true);
        Task::batch([self.reveal_cursor(), operation::focus(Id::new(EDITOR_ID))])
    }

    /// Inline variable (`⌥⌘N`) at the caret.
    pub(crate) fn inline_variable(&mut self) -> Task<Message> {
        let Some(doc) = self.doc().filter(|d| d.is_php()) else {
            return Task::none();
        };
        match refactor::inline_variable(&doc.text, doc.offset(doc.cursor())) {
            Ok(plan) => self.apply_local(plan),
            Err(error) => {
                self.fail(error);
                Task::none()
            }
        }
    }

    /// Applies a planned project-wide refactoring (or shows its error in
    /// the prompt).
    pub(crate) fn apply_refactor_plan(
        &mut self,
        plan: Result<RenamePlan, String>,
    ) -> Task<Message> {
        match plan {
            Ok(plan) => {
                self.refactor_prompt = None;
                self.apply_rename(Ok(plan))
            }
            Err(error) => {
                if let Some(p) = &mut self.refactor_prompt {
                    p.error = Some(error);
                }
                Task::none()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_changes_should_match_by_name_and_default_new_ones() {
        let changes = parameter_changes(
            "string $name, int $times",
            "int $times, string $name, bool $loud = false, ?User $by",
        );
        let froms: Vec<Option<usize>> = changes.iter().map(|c| c.from).collect();
        assert_eq!(froms, [Some(1), Some(0), None, None]);
        assert_eq!(changes[2].default_argument, "false");
        assert_eq!(changes[3].default_argument, "null");
        assert_eq!(
            split_parameters("array $a = [1, 2], string $s = 'x,y'"),
            ["array $a = [1, 2]", "string $s = 'x,y'"]
        );
    }

    #[test]
    fn folder_for_namespace_should_follow_psr4() {
        let root = std::env::temp_dir().join(format!("phpthunder-psr4-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("composer.json"),
            r#"{"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        )
        .unwrap();
        assert_eq!(
            folder_for_namespace(&root, "App\\Models"),
            Some(root.join("app/Models"))
        );
        assert_eq!(folder_for_namespace(&root, "Other"), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
