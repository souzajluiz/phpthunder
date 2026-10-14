//! LSP front end.
//!
//! Indexing runs in the background after `initialized`. Until it finishes the
//! index is partial: definition, hover, and completion answer from what is
//! indexed so far, while find usages, which needs a complete index to be
//! correct, waits until indexing is done. Diagnostics are published for open
//! files once indexing is done (a partial index would report missing
//! classes that are merely not indexed yet), then after every change.

use crate::convert;
use parking_lot::{Mutex, RwLock};
use phpthunder::analysis::{self, Candidate, Definition, KEYWORD_DETAIL};
use phpthunder::index::{self, Index, index_source};
use phpthunder::inspection;
use phpthunder::laravel::{self, LaravelProject};
use phpthunder::refactor;
use phpthunder::symbol::{Kind, Symbol};
use phpthunder::syntax::{LineIndex, Pos, Span};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress;
use tower_lsp::lsp_types::request::WorkDoneProgressCreate;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

/// State shared between the async LSP handlers and blocking worker tasks.
struct Shared {
    /// Project index; written by reindexing, read by every request.
    index: RwLock<Index>,
    /// Open editor buffers; they take precedence over disk content.
    docs: Mutex<HashMap<PathBuf, String>>,
    /// Route names, views, config, translation and env keys of each
    /// Laravel workspace root, for completion inside key strings.
    laravel: RwLock<Vec<(PathBuf, LaravelProject)>>,
}

impl Shared {
    /// Current content of `path`: the open editor buffer if any, else the file on
    /// disk. `None` when neither exists, which reindexing treats as a deletion.
    fn load(&self, path: &Path) -> Option<String> {
        if let Some(text) = self.docs.lock().get(path) {
            return Some(text.clone());
        }
        std::fs::read_to_string(path).ok()
    }
}

/// The language server. `tower_lsp::LspService::new(Backend::new)` builds it (see
/// the `phpthunder` binary); each [`LanguageServer`] method handles one LSP
/// request or notification.
pub struct Backend {
    /// Connection to the editor, for progress, logs, and registrations.
    client: Client,
    /// Index and open buffers, shared with blocking workers.
    shared: Arc<Shared>,
    /// Workspace folders from `initialize`; scanned by the initial indexing.
    roots: Mutex<Vec<PathBuf>>,
    /// Becomes `true` when the initial scan finishes; find usages waits for it.
    ready: watch::Sender<bool>,
}

impl Backend {
    /// Creates a server with an empty index. Nothing is indexed until the client sends `initialized`.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            shared: Arc::new(Shared {
                index: RwLock::new(Index::default()),
                docs: Mutex::new(HashMap::new()),
                laravel: RwLock::new(Vec::new()),
            }),
            roots: Mutex::new(Vec::new()),
            ready: watch::channel(false).0,
        }
    }

    /// Runs `f` on tokio's blocking pool, so parsing and index work never stall
    /// the async LSP loop.
    ///
    /// # Panics
    ///
    /// If `f` panics.
    async fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Shared) -> T + Send + 'static,
    ) -> T {
        let shared = self.shared.clone();
        tokio::task::spawn_blocking(move || f(&shared))
            .await
            .expect("worker panicked")
    }

    /// Re-reads `path` (buffer or disk) and replaces its index entry, or removes it
    /// when the file is gone. Called on open, change, close, and watched-file
    /// events.
    async fn reindex(&self, path: PathBuf) {
        self.blocking(move |s| match s.load(&path) {
            Some(text) => {
                let entry = index_source(&text);
                s.index.write().update(path, entry);
            }
            None => s.index.write().remove(&path),
        })
        .await;
    }

    /// Inspects `path` (its open buffer) and publishes the diagnostics; an
    /// empty set clears them. Skipped until the initial scan is done.
    async fn publish_diagnostics(&self, path: PathBuf) {
        if !*self.ready.borrow() {
            return;
        }
        let Ok(uri) = Url::from_file_path(&path) else {
            return;
        };
        let diagnostics = self
            .blocking(move |s| {
                let open = s.docs.lock().get(&path).cloned();
                let Some(text) = open.filter(|_| path.extension().is_some_and(|e| e == "php"))
                else {
                    return Vec::new();
                };
                inspection::diagnostics(&s.index.read(), &text)
                    .iter()
                    .map(convert::diagnostic)
                    .collect()
            })
            .await;
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    /// Resolves once the initial workspace scan has finished; immediately afterwards.
    async fn wait_until_indexed(&self) {
        let mut rx = self.ready.subscribe();
        let _ = rx.wait_for(|ready| *ready).await;
    }

    /// Indexes every PHP file under the workspace roots, reporting work-done
    /// progress when the client supports it. Sets `ready` and logs the elapsed
    /// time when done. Runs once, from `initialized`.
    async fn index_workspace(&self) {
        let roots = self.roots.lock().clone();
        let token = NumberOrString::String("phpthunder/indexing".into());
        let progress_supported = self
            .client
            .send_request::<WorkDoneProgressCreate>(WorkDoneProgressCreateParams {
                token: token.clone(),
            })
            .await
            .is_ok();
        let report = |value: WorkDoneProgress| {
            let client = self.client.clone();
            let token = token.clone();
            async move {
                if progress_supported {
                    client
                        .send_notification::<Progress>(ProgressParams {
                            token,
                            value: ProgressParamsValue::WorkDone(value),
                        })
                        .await;
                }
            }
        };
        report(WorkDoneProgress::Begin(WorkDoneProgressBegin {
            title: "Indexing PHP files".into(),
            percentage: Some(0),
            ..Default::default()
        }))
        .await;

        let (tx, mut rx) = mpsc::unbounded_channel::<(usize, usize)>();
        let started = std::time::Instant::now();
        let job = self.blocking(move |s| {
            // Built-ins let inspections tell a missing function from `strlen`.
            if let Ok(path) = index::write_builtins() {
                s.index
                    .write()
                    .update(path, index_source(index::BUILTINS_SOURCE));
            }
            *s.laravel.write() = roots
                .iter()
                .filter_map(|r| Some((r.clone(), laravel::scan_project(r)?)))
                .collect();
            let files: Vec<PathBuf> = roots.iter().flat_map(|r| index::php_files(r)).collect();
            let total = files.len();
            index::index_files(&s.index, &files, |done, total| {
                let _ = tx.send((done, total));
            });
            total
        });
        tokio::pin!(job);
        let total = loop {
            tokio::select! {
                total = &mut job => break total,
                Some((done, total)) = rx.recv() => {
                    report(WorkDoneProgress::Report(WorkDoneProgressReport {
                        message: Some(format!("{done}/{total} files")),
                        percentage: Some((done * 100 / total.max(1)) as u32),
                        ..Default::default()
                    }))
                    .await;
                }
            }
        };
        report(WorkDoneProgress::End(WorkDoneProgressEnd {
            message: Some(format!("{total} files indexed")),
        }))
        .await;
        self.ready.send_replace(true);
        let open: Vec<PathBuf> = self.shared.docs.lock().keys().cloned().collect();
        for path in open {
            self.publish_diagnostics(path).await;
        }
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "phpthunder: indexed {total} PHP files in {:.2?}",
                    started.elapsed()
                ),
            )
            .await;
    }
}

/// Most completion proposals returned at once.
const MAX_COMPLETIONS: usize = 200;

/// Converts a [`Span`] (already in LSP line/UTF-16 coordinates) to an LSP range.
fn to_range(span: Span) -> Range {
    Range {
        start: Position::new(span.start.line, span.start.col),
        end: Position::new(span.end.line, span.end.col),
    }
}

/// LSP location of `span` in `path`; `None` if the path cannot become a `file://` URL.
fn location(path: &Path, span: Span) -> Option<Location> {
    Some(Location::new(
        Url::from_file_path(path).ok()?,
        to_range(span),
    ))
}

/// File-system path of a `file://` URI; `None` for other schemes (untitled or remote buffers), which the server ignores.
fn path_of(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

/// LSP symbol kind for document and workspace symbols; traits are shown as classes.
fn symbol_kind(kind: Kind) -> SymbolKind {
    match kind {
        Kind::Class => SymbolKind::CLASS,
        Kind::Interface => SymbolKind::INTERFACE,
        Kind::Trait => SymbolKind::CLASS,
        Kind::Enum => SymbolKind::ENUM,
        Kind::Function => SymbolKind::FUNCTION,
        Kind::Constant | Kind::ClassConstant => SymbolKind::CONSTANT,
        Kind::Method => SymbolKind::METHOD,
        Kind::Property => SymbolKind::PROPERTY,
        Kind::EnumCase => SymbolKind::ENUM_MEMBER,
    }
}

/// LSP completion icon for a candidate; local variables (`Property` with detail `variable`) get the variable icon.
fn completion_kind(c: &Candidate) -> CompletionItemKind {
    match c.kind {
        Kind::Class | Kind::Trait => CompletionItemKind::CLASS,
        Kind::Interface => CompletionItemKind::INTERFACE,
        Kind::Enum => CompletionItemKind::ENUM,
        Kind::Function => CompletionItemKind::FUNCTION,
        Kind::Method => CompletionItemKind::METHOD,
        Kind::Property if c.detail == "variable" => CompletionItemKind::VARIABLE,
        Kind::Property => CompletionItemKind::PROPERTY,
        Kind::Constant | Kind::ClassConstant => CompletionItemKind::CONSTANT,
        Kind::EnumCase => CompletionItemKind::ENUM_MEMBER,
    }
}

/// Markdown hover for a declaration: its signature in a PHP code block, the
/// qualified name (`Owner::member` for members), then the PHPDoc summary.
fn hover_text(def: &Definition) -> String {
    let s: &Symbol = &def.symbol;
    let owner = if s.kind.is_member() {
        format!("{}::", s.fqn)
    } else {
        String::new()
    };
    let name = if s.kind.is_member() {
        s.name.clone()
    } else {
        s.fqn.clone()
    };
    let doc = if s.doc.is_empty() {
        String::new()
    } else {
        format!("\n\n{}", s.doc)
    };
    format!("```php\n<?php\n{}\n```\n`{owner}{name}`{doc}", s.signature)
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let mut roots: Vec<PathBuf> = params
            .workspace_folders
            .unwrap_or_default()
            .iter()
            .filter_map(|f| path_of(&f.uri))
            .collect();
        #[allow(deprecated)]
        if roots.is_empty() {
            roots.extend(params.root_uri.as_ref().and_then(path_of));
        }
        *self.roots.lock() = roots;

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "phpthunder".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ">".into(),
                        ":".into(),
                        "$".into(),
                        "'".into(),
                        "\"".into(),
                        ".".into(),
                    ]),
                    ..Default::default()
                }),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::REFACTOR_REWRITE,
                            CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                        ]),
                        ..Default::default()
                    },
                )),
                rename_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let watcher = Registration {
            id: "phpthunder/watch-php".into(),
            method: "workspace/didChangeWatchedFiles".into(),
            register_options: serde_json_value(DidChangeWatchedFilesRegistrationOptions {
                watchers: vec![FileSystemWatcher {
                    glob_pattern: GlobPattern::String("**/*.php".into()),
                    kind: None,
                }],
            }),
        };
        let _ = self.client.register_capability(vec![watcher]).await;
        self.index_workspace().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let Some(path) = path_of(&params.text_document.uri) else {
            return;
        };
        self.shared
            .docs
            .lock()
            .insert(path.clone(), params.text_document.text);
        self.reindex(path.clone()).await;
        self.publish_diagnostics(path).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Some(path) = path_of(&params.text_document.uri) else {
            return;
        };
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };
        self.shared.docs.lock().insert(path.clone(), change.text);
        self.reindex(path.clone()).await;
        self.publish_diagnostics(path).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let Some(path) = path_of(&params.text_document.uri) else {
            return;
        };
        self.shared.docs.lock().remove(&path);
        self.reindex(path.clone()).await;
        // Closed files keep no diagnostics.
        if let Ok(uri) = Url::from_file_path(&path) {
            self.client.publish_diagnostics(uri, Vec::new(), None).await;
        }
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        for change in params.changes {
            if let Some(path) = path_of(&change.uri) {
                self.reindex(path).await;
            }
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let pos = params.text_document_position_params;
        let Some(path) = path_of(&pos.text_document.uri) else {
            return Ok(None);
        };
        let defs = self
            .blocking(move |s| {
                let src = s.load(&path)?;
                let byte = LineIndex::new(&src).offset(
                    &src,
                    Pos {
                        line: pos.position.line,
                        col: pos.position.character,
                    },
                );
                Some(analysis::definitions_at(&s.index.read(), &src, byte))
            })
            .await
            .unwrap_or_default();
        let locations: Vec<Location> = defs
            .iter()
            .filter_map(|d| location(&d.path, d.symbol.name_span))
            .collect();
        Ok((!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations)))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        self.wait_until_indexed().await;
        let pos = params.text_document_position;
        let include_declaration = params.context.include_declaration;
        let Some(path) = path_of(&pos.text_document.uri) else {
            return Ok(None);
        };
        let usages = self
            .blocking(move |s| {
                let src = s.load(&path)?;
                let byte = LineIndex::new(&src).offset(
                    &src,
                    Pos {
                        line: pos.position.line,
                        col: pos.position.character,
                    },
                );
                let index = s.index.read();
                let defs = analysis::definitions_at(&index, &src, byte);
                Some(analysis::find_references(&index, &defs, |p| s.load(p)))
            })
            .await
            .unwrap_or_default();
        Ok(Some(
            usages
                .iter()
                .filter(|u| include_declaration || !u.is_declaration)
                .filter_map(|u| location(&u.path, u.span))
                .collect(),
        ))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let pos = params.text_document_position_params;
        let Some(path) = path_of(&pos.text_document.uri) else {
            return Ok(None);
        };
        let defs = self
            .blocking(move |s| {
                let src = s.load(&path)?;
                let byte = LineIndex::new(&src).offset(
                    &src,
                    Pos {
                        line: pos.position.line,
                        col: pos.position.character,
                    },
                );
                Some(analysis::definitions_at(&s.index.read(), &src, byte))
            })
            .await
            .unwrap_or_default();
        Ok(defs.first().map(|d| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover_text(d),
            }),
            range: None,
        }))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let pos = params.text_document_position;
        let Some(path) = path_of(&pos.text_document.uri) else {
            return Ok(None);
        };
        let items = self
            .blocking(move |s| {
                let src = s.load(&path)?;
                let lines = LineIndex::new(&src);
                let byte = lines.offset(
                    &src,
                    Pos {
                        line: pos.position.line,
                        col: pos.position.character,
                    },
                );
                // Inside a Laravel key string (`route('…')`): the keys.
                if let Some(key) = laravel::key_at(&src, byte) {
                    let projects = s.laravel.read();
                    let project = projects.iter().find(|(root, _)| path.starts_with(root));
                    if let Some((_, project)) = project {
                        let range = to_range(lines.span(&src, key.start, byte));
                        let keys: Vec<Candidate> = project
                            .keys(key.kind)
                            .iter()
                            .map(|k| Candidate {
                                label: k.name.clone(),
                                kind: Kind::Constant,
                                detail: k.detail.clone(),
                                insert_text: k.name.clone(),
                                import: None,
                            })
                            .collect();
                        let ranked =
                            analysis::rank_candidates(&key.prefix, keys, |_| 0, MAX_COMPLETIONS);
                        return Some(
                            ranked
                                .into_iter()
                                .enumerate()
                                .map(|(i, r)| CompletionItem {
                                    label: r.candidate.label.clone(),
                                    kind: Some(CompletionItemKind::VALUE),
                                    detail: Some(r.candidate.detail),
                                    sort_text: Some(format!("{i:04}")),
                                    filter_text: Some(r.candidate.label.clone()),
                                    text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                                        range,
                                        r.candidate.insert_text,
                                    ))),
                                    ..Default::default()
                                })
                                .collect(),
                        );
                    }
                }
                let mut candidates = analysis::complete(&s.index.read(), &src, byte);
                let mut start = analysis::ident_start(&src, byte);
                if start > 0 && src.as_bytes()[start - 1] == b'$' {
                    start -= 1;
                }
                if byte > start && !src[start..byte].starts_with('$') {
                    candidates.extend(analysis::keyword_candidates(&src, byte));
                }
                // Ranked like the IDE (match quality, proximity, length);
                // `sort_text` keeps that order in the client.
                let ranked = analysis::rank_candidates(
                    &src[start..byte],
                    candidates,
                    |_| 0,
                    MAX_COMPLETIONS,
                );
                let items = ranked
                    .into_iter()
                    .enumerate()
                    .map(|(i, r)| {
                        let c = r.candidate;
                        // Replace the typed prefix; keep `$` only when the candidate has it.
                        let from =
                            if src[start..].starts_with('$') && !c.insert_text.starts_with('$') {
                                start + 1
                            } else {
                                start
                            };
                        let edit_range = to_range(lines.span(&src, from, byte));
                        let kind = if c.detail == KEYWORD_DETAIL {
                            CompletionItemKind::KEYWORD
                        } else {
                            completion_kind(&c)
                        };
                        CompletionItem {
                            kind: Some(kind),
                            detail: Some(c.detail.clone()),
                            sort_text: Some(format!("{i:04}")),
                            filter_text: Some(c.label.clone()),
                            text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                                edit_range,
                                c.insert_text.clone(),
                            ))),
                            additional_text_edits: c.import.as_ref().map(|(at, text)| {
                                let p = lines.pos(&src, *at);
                                let p = Position::new(p.line, p.col);
                                vec![TextEdit::new(Range::new(p, p), text.clone())]
                            }),
                            label: c.label,
                            ..Default::default()
                        }
                    })
                    .collect::<Vec<_>>();
                Some(items)
            })
            .await
            .unwrap_or_default();
        // Incomplete: the engine caps and fuzzy-matches names, so a longer
        // prefix can bring proposals this list does not have.
        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: true,
            items,
        })))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let Some(path) = path_of(&params.text_document.uri) else {
            return Ok(None);
        };
        let range = params.range;
        let only = params.context.only.clone();
        let actions = self
            .blocking(move |s| {
                let src = s.load(&path)?;
                let index = s.index.read();
                let (start, end) = convert::byte_range(&src, range);
                let wants = |kind: &CodeActionKind| {
                    only.as_ref()
                        .is_none_or(|o| o.iter().any(|k| kind.as_str().starts_with(k.as_str())))
                };
                let mut actions = Vec::new();
                if wants(&CodeActionKind::QUICKFIX) {
                    for d in inspection::diagnostics(&index, &src)
                        .into_iter()
                        .filter(|d| d.start <= end && start <= d.end)
                    {
                        for fix in inspection::quick_fixes(&index, &src, &d, |p| s.load(p)) {
                            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                                title: fix.title.clone(),
                                kind: Some(CodeActionKind::QUICKFIX),
                                diagnostics: Some(vec![convert::diagnostic(&d)]),
                                edit: Some(convert::workspace_edit(&path, &fix.edits, |p| {
                                    s.load(p)
                                })),
                                ..Default::default()
                            }));
                        }
                    }
                }
                if wants(&CodeActionKind::REFACTOR_REWRITE) {
                    for fix in phpthunder::intention::intentions(&index, &src, start, end) {
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: fix.title.clone(),
                            kind: Some(CodeActionKind::REFACTOR_REWRITE),
                            edit: Some(convert::workspace_edit(&path, &fix.edits, |p| s.load(p))),
                            ..Default::default()
                        }));
                    }
                }
                if wants(&CodeActionKind::SOURCE_ORGANIZE_IMPORTS) {
                    let organized = refactor::optimize_imports(&index, &src);
                    if organized != src {
                        let edit = inspection::Edit {
                            path: None,
                            start: 0,
                            end: src.len(),
                            text: organized,
                        };
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: "Optimize imports".into(),
                            kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
                            edit: Some(convert::workspace_edit(&path, &[edit], |p| s.load(p))),
                            ..Default::default()
                        }));
                    }
                }
                Some(actions)
            })
            .await;
        Ok(actions.filter(|a| !a.is_empty()))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        self.wait_until_indexed().await;
        let pos = params.text_document_position;
        let Some(path) = path_of(&pos.text_document.uri) else {
            return Ok(None);
        };
        let new_name = params.new_name;
        let result = self
            .blocking(move |s| {
                let src = s.load(&path).ok_or_else(|| "file not found".to_string())?;
                let lines = LineIndex::new(&src);
                let byte = lines.offset(
                    &src,
                    Pos {
                        line: pos.position.line,
                        col: pos.position.character,
                    },
                );
                let plan =
                    refactor::rename(&s.index.read(), &path, &src, byte, &new_name, |p| s.load(p))?;
                Ok::<_, String>(convert::rename_edit(&plan, |p| s.load(p)))
            })
            .await;
        match result {
            Ok(edit) => Ok(Some(edit)),
            Err(message) => Err(tower_lsp::jsonrpc::Error::invalid_params(message)),
        }
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let pos = params.text_document_position_params;
        let Some(path) = path_of(&pos.text_document.uri) else {
            return Ok(None);
        };
        let help = self
            .blocking(move |s| {
                let src = s.load(&path)?;
                let lines = LineIndex::new(&src);
                let byte = lines.offset(
                    &src,
                    Pos {
                        line: pos.position.line,
                        col: pos.position.character,
                    },
                );
                refactor::signature_help(&s.index.read(), &src, byte)
            })
            .await;
        Ok(help.map(|h| SignatureHelp {
            signatures: vec![SignatureInformation {
                label: h.label,
                documentation: None,
                parameters: Some(
                    h.params
                        .into_iter()
                        .map(|p| ParameterInformation {
                            label: ParameterLabel::Simple(p),
                            documentation: None,
                        })
                        .collect(),
                ),
                active_parameter: u32::try_from(h.active).ok(),
            }],
            active_signature: Some(0),
            active_parameter: None,
        }))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(path) = path_of(&params.text_document.uri) else {
            return Ok(None);
        };
        let uri = params.text_document.uri;
        let symbols = self
            .blocking(move |s| {
                let index = s.index.read();
                index
                    .file_symbols(&path)
                    .iter()
                    .map(|sym| {
                        #[allow(deprecated)]
                        SymbolInformation {
                            name: sym.name.clone(),
                            kind: symbol_kind(sym.kind),
                            tags: None,
                            deprecated: None,
                            location: Location::new(uri.clone(), to_range(sym.span)),
                            container_name: sym.kind.is_member().then(|| sym.fqn.clone()),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .await;
        Ok(Some(DocumentSymbolResponse::Flat(symbols)))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let symbols = self
            .blocking(move |s| {
                let index = s.index.read();
                index
                    .search(&params.query, 500)
                    .into_iter()
                    .filter_map(|(path, sym)| {
                        #[allow(deprecated)]
                        Some(SymbolInformation {
                            name: sym.name.clone(),
                            kind: symbol_kind(sym.kind),
                            tags: None,
                            deprecated: None,
                            location: location(path, sym.name_span)?,
                            container_name: sym.fqn.rsplit_once('\\').map(|(ns, _)| ns.to_string()),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .await;
        Ok(Some(symbols))
    }
}

/// Serializes registration options for `register_capability`; `None` on failure, so the registration just carries no options.
fn serde_json_value<T: serde::Serialize>(value: T) -> Option<serde_json::Value> {
    serde_json::to_value(value).ok()
}
