//! Go to declaration for framework strings: Twig template names open the
//! template, WordPress hook names jump between `do_action` /
//! `apply_filters` and the `add_action` / `add_filter` listeners.

use super::*;
use crate::frameworks::{self, HookSide};
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;
use phpthunder::syntax::{LineIndex, Pos};
use std::path::PathBuf;

/// Every call on `side` for `hook` in the PHP files `paths`, as
/// positions.
fn hook_sites_in(paths: &[PathBuf], hook: &str, side: HookSide) -> Vec<(PathBuf, Pos)> {
    let mut found = Vec::new();
    for path in paths {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if !text.contains(hook) {
            continue;
        }
        let lines = LineIndex::new(&text);
        found.extend(
            frameworks::hook_sites(&text, hook, side)
                .into_iter()
                .map(|at| (path.clone(), lines.pos(&text, at))),
        );
    }
    found
}

impl App {
    /// Jumps to the target of the `$ref` on the caret's line (OpenAPI,
    /// JSON Schema), in this document or the relative file it names;
    /// `None` when the line has no `$ref`.
    pub(crate) fn ref_definition(&mut self) -> Option<Task<Message>> {
        use crate::filetype::FileType;
        let doc = self.doc()?;
        if !matches!(doc.file_type, FileType::Json | FileType::Yaml) {
            return None;
        }
        let offset = doc.offset(doc.cursor());
        let (file, pointer) = crate::structured::ref_at(&doc.text, offset)?;
        let (path, text) = if file.is_empty() {
            (doc.path.clone(), doc.text.clone())
        } else {
            let path = doc.path.parent()?.join(&file);
            let Ok(text) = std::fs::read_to_string(&path) else {
                self.notice = Some(tr!("notice-no-declaration"));
                return Some(Task::none());
            };
            (path, text)
        };
        let yaml = FileType::detect(&path, None) == FileType::Yaml;
        let Some(at) = crate::structured::resolve_pointer(&text, &pointer, yaml) else {
            self.notice = Some(tr!("notice-no-declaration"));
            return Some(Task::none());
        };
        let pos = LineIndex::new(&text).pos(&text, at);
        self.close_overlays();
        self.record_nav();
        Some(self.open(path, Some(pos)))
    }

    /// Copies the JSONPath (JSON, YAML) or XPath (XML, HTML) of the caret
    /// and shows it.
    pub(crate) fn copy_structure_path(&mut self) -> Task<Message> {
        use crate::filetype::FileType;
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let path = match doc.file_type {
            FileType::Json => {
                crate::structured::json_path(&crate::structured::json_path_at(&doc.text, offset))
            }
            FileType::Yaml => {
                crate::structured::json_path(&crate::structured::yaml_path_at(&doc.text, offset))
            }
            FileType::Xml | FileType::Html => crate::structured::xpath_at(&doc.text, offset),
            _ => {
                self.notice = Some(tr!("notice-structure-path-unsupported"));
                return Task::none();
            }
        };
        self.notice = Some(tr!("notice-structure-path-copied", path = path.as_str()));
        iced::clipboard::write(path)
    }

    /// The declaration task for a Twig name or WordPress hook at the caret;
    /// `None` when the caret is on neither, so the usual lookup runs.
    pub(crate) fn framework_definition(&mut self) -> Option<Task<Message>> {
        let root = self.tree.root.clone()?;
        let doc = self.doc()?;
        let offset = doc.offset(doc.cursor());
        if let Some(name) = frameworks::twig_name_at(&doc.text, offset) {
            let name = name.to_string();
            let found = frameworks::resolve_twig(&root, &self.project_files, &name)
                .into_iter()
                .map(|p| (p, Pos::default()))
                .collect();
            return Some(self.open_found(tr!("twig-templates-title", name = name), found));
        }
        let (hook, side) = frameworks::hook_at(&doc.text, offset)?;
        let hook = hook.to_string();
        let wanted = match side {
            HookSide::Listen => HookSide::Fire,
            HookSide::Fire => HookSide::Listen,
        };
        let paths: Vec<PathBuf> = self
            .project_files
            .iter()
            .chain(&self.vendor_files)
            .filter(|p| p.extension().is_some_and(|e| e == "php"))
            .cloned()
            .collect();
        let title = tr!("hook-sites-title", name = hook.as_str());
        // Reading every PHP file stays off the UI thread.
        Some(Task::perform(
            async move {
                tokio::task::spawn_blocking(move || hook_sites_in(&paths, &hook, wanted))
                    .await
                    .unwrap_or_default()
            },
            move |found| Message::FrameworkSites(title, found),
        ))
    }

    /// Opens the only location of `found`, lists several in a picker
    /// titled `title`, or says nothing was found.
    pub(crate) fn open_found(
        &mut self,
        title: String,
        found: Vec<(PathBuf, Pos)>,
    ) -> Task<Message> {
        match found.len() {
            0 => {
                self.notice = Some(tr!("notice-no-declaration"));
                Task::none()
            }
            1 => {
                let Some((path, pos)) = found.into_iter().next() else {
                    return Task::none();
                };
                self.close_overlays();
                self.record_nav();
                self.open(path, Some(pos))
            }
            _ => {
                let root = self.tree.root.clone();
                let items = found
                    .into_iter()
                    .map(|(path, pos)| {
                        let rel = root
                            .as_ref()
                            .and_then(|r| path.strip_prefix(r).ok())
                            .unwrap_or(&path)
                            .display()
                            .to_string();
                        Item {
                            badge: "→",
                            title: crate::app::file_name(&path),
                            matched: Vec::new(),
                            detail: format!("{rel}:{}", pos.line + 1),
                            target: Target::Symbol(path, pos),
                        }
                    })
                    .collect();
                self.open_choice(title, items)
            }
        }
    }
}
