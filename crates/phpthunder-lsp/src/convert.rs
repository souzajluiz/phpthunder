//! Converting engine results to LSP types: diagnostics, text edits across
//! files, and rename plans with their file rename.

use phpthunder::inspection::{Code, Diagnostic as EngineDiagnostic, Edit, Severity};
use phpthunder::refactor::RenamePlan;
use phpthunder::syntax::{LineIndex, Pos};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::*;

/// Byte offsets of an LSP range in `src`.
pub fn byte_range(src: &str, range: Range) -> (usize, usize) {
    let lines = LineIndex::new(src);
    let at = |p: Position| {
        lines.offset(
            src,
            Pos {
                line: p.line,
                col: p.character,
            },
        )
    };
    (at(range.start), at(range.end))
}

/// LSP range of the byte offsets `start..end` in `src`.
pub fn lsp_range(lines: &LineIndex, src: &str, start: usize, end: usize) -> Range {
    let span = lines.span(src, start, end);
    Range::new(
        Position::new(span.start.line, span.start.col),
        Position::new(span.end.line, span.end.col),
    )
}

/// Stable code shown with a diagnostic (`undefined-class`, …).
fn code_name(code: &Code) -> &'static str {
    match code {
        Code::UndefinedClass(_) => "undefined-class",
        Code::UndefinedFunction(_) => "undefined-function",
        Code::UndefinedMethod { .. } => "undefined-method",
        Code::UndefinedProperty { .. } => "undefined-property",
        Code::UnusedImport(_) => "unused-import",
        Code::UndefinedVariable(_) => "undefined-variable",
        Code::UnusedVariable(_) => "unused-variable",
        Code::MissingReturn(_) => "missing-return",
        Code::ReturnTypeMismatch { .. } => "return-type-mismatch",
        Code::UnknownLaravelKey { .. } => "unknown-laravel-key",
        Code::SimplifyOrderBy => "simplify-order-by",
        Code::External(_) => "external",
    }
}

/// An engine diagnostic as LSP reports it. Unused imports are tagged
/// `UNNECESSARY`, so editors fade them.
pub fn diagnostic(d: &EngineDiagnostic) -> Diagnostic {
    let span = d.span;
    Diagnostic {
        range: Range::new(
            Position::new(span.start.line, span.start.col),
            Position::new(span.end.line, span.end.col),
        ),
        severity: Some(match d.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(code_name(&d.code).into())),
        source: Some("phpthunder".into()),
        message: d.message.clone(),
        tags: matches!(d.code, Code::UnusedImport(_) | Code::UnusedVariable(_))
            .then(|| vec![DiagnosticTag::UNNECESSARY]),
        ..Default::default()
    }
}

/// Text edits of one file as LSP edits, with ranges computed from `text`.
fn text_edits(text: &str, edits: &[Edit]) -> Vec<TextEdit> {
    let lines = LineIndex::new(text);
    edits
        .iter()
        .map(|e| TextEdit::new(lsp_range(&lines, text, e.start, e.end), e.text.clone()))
        .collect()
}

/// A workspace edit from engine edits; an edit without a path applies to
/// `current`. `load` gives each file's text (open buffer or disk); files it
/// cannot load are skipped.
pub fn workspace_edit(
    current: &Path,
    edits: &[Edit],
    load: impl Fn(&Path) -> Option<String>,
) -> WorkspaceEdit {
    let mut by_file: BTreeMap<PathBuf, Vec<Edit>> = BTreeMap::new();
    for edit in edits {
        let path = edit.path.clone().unwrap_or_else(|| current.to_path_buf());
        by_file.entry(path).or_default().push(edit.clone());
    }
    let mut changes = HashMap::new();
    for (path, edits) in by_file {
        let (Some(text), Ok(uri)) = (load(&path), Url::from_file_path(&path)) else {
            continue;
        };
        changes.insert(uri, text_edits(&text, &edits));
    }
    WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    }
}

/// A rename plan as a workspace edit: text edits per file, then the file
/// rename when the class file follows the class.
pub fn rename_edit(plan: &RenamePlan, load: impl Fn(&Path) -> Option<String>) -> WorkspaceEdit {
    let mut operations = Vec::new();
    for (path, edits) in &plan.edits {
        let (Some(text), Ok(uri)) = (load(path), Url::from_file_path(path)) else {
            continue;
        };
        operations.push(DocumentChangeOperation::Edit(TextDocumentEdit {
            text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
            edits: text_edits(&text, edits)
                .into_iter()
                .map(OneOf::Left)
                .collect(),
        }));
    }
    if let Some((from, to)) = &plan.file_rename
        && let (Ok(old_uri), Ok(new_uri)) = (Url::from_file_path(from), Url::from_file_path(to))
    {
        operations.push(DocumentChangeOperation::Op(ResourceOp::Rename(
            RenameFile {
                old_uri,
                new_uri,
                options: None,
                annotation_id: None,
            },
        )));
    }
    WorkspaceEdit {
        document_changes: Some(DocumentChanges::Operations(operations)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_edit_should_place_edits_by_line_and_column() {
        let path = PathBuf::from("/tmp/phpthunder-lsp-edit.php");
        let text = "<?php\nuse A;\n".to_string();
        let edits = vec![Edit {
            path: None,
            start: 6,
            end: 13,
            text: String::new(),
        }];
        let edit = workspace_edit(&path, &edits, |_| Some(text.clone()));
        let changes = edit.changes.unwrap();
        let file = &changes[&Url::from_file_path(&path).unwrap()];
        assert_eq!(
            file[0].range,
            Range::new(Position::new(1, 0), Position::new(2, 0))
        );
    }
}
