//! LSP front end.
//!
//! Indexing runs in the background after `initialized`. Until it finishes the
//! index is partial: definition, hover, and completion answer from what is
//! indexed so far, while find usages, which needs a complete index to be
//! correct, waits until indexing is done.

use parking_lot::{Mutex, RwLock};
use phpthunder::analysis::{self, Candidate, Definition};
use phpthunder::index::{self, Index, index_source};
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
                    trigger_characters: Some(vec![">".into(), ":".into(), "$".into()]),
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
        self.reindex(path).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Some(path) = path_of(&params.text_document.uri) else {
            return;
        };
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };
        self.shared.docs.lock().insert(path.clone(), change.text);
        self.reindex(path).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let Some(path) = path_of(&params.text_document.uri) else {
            return;
        };
        self.shared.docs.lock().remove(&path);
        self.reindex(path).await;
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
                let candidates = analysis::complete(&s.index.read(), &src, byte);
                let mut start = analysis::ident_start(&src, byte);
                if start > 0 && src.as_bytes()[start - 1] == b'$' {
                    start -= 1;
                }
                let items = candidates
                    .into_iter()
                    .map(|c| {
                        // Replace the typed prefix; keep `$` only when the candidate has it.
                        let from =
                            if src[start..].starts_with('$') && !c.insert_text.starts_with('$') {
                                start + 1
                            } else {
                                start
                            };
                        let edit_range = to_range(lines.span(&src, from, byte));
                        CompletionItem {
                            kind: Some(completion_kind(&c)),
                            detail: Some(c.detail.clone()),
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
        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: !*self.ready.borrow(),
            items,
        })))
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
