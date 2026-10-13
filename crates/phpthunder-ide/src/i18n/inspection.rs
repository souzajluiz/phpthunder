//! Phrasing engine diagnostics and quick fixes in the UI language. The
//! engine keeps English messages for the language server; the IDE rebuilds
//! them from their machine-readable codes.

use crate::tr;
use phpthunder::inspection::{Code, Diagnostic, Fix, FixKind};

/// Last segment of a class FQN (`App\Models\User` -> `User`).
fn short(fqn: &str) -> &str {
    fqn.rsplit('\\').next().unwrap_or(fqn)
}

/// The diagnostic's message in the UI language. Messages from external
/// analysers (PHPStan) are passed through unchanged.
///
/// Used by the Problems panel and the hover popup.
pub fn diagnostic_message(diagnostic: &Diagnostic) -> String {
    match &diagnostic.code {
        Code::UndefinedClass(fqn) => tr!("diagnostic-undefined-class", name = short(fqn)),
        Code::UndefinedFunction(name) => tr!("diagnostic-undefined-function", name = name.as_str()),
        Code::UndefinedMethod { class, name, .. } => {
            tr!(
                "diagnostic-undefined-method",
                name = name.as_str(),
                class = short(class)
            )
        }
        Code::UndefinedProperty { class, name } => {
            tr!(
                "diagnostic-undefined-property",
                name = name.as_str(),
                class = short(class)
            )
        }
        Code::UnusedImport(fqn) => tr!("diagnostic-unused-import", name = fqn.as_str()),
        Code::UndefinedVariable(name) => tr!("diagnostic-undefined-variable", name = name.as_str()),
        Code::UnusedVariable(name) => tr!("diagnostic-unused-variable", name = name.as_str()),
        Code::MissingReturn(ty) => tr!("diagnostic-missing-return", ty = ty.as_str()),
        Code::ReturnTypeMismatch { expected, found } => tr!(
            "diagnostic-return-mismatch",
            expected = expected.as_str(),
            found = found.as_str()
        ),
        Code::UnknownLaravelKey { kind, name } => tr!(
            "diagnostic-unknown-laravel-key",
            kind = format!("{kind:?}"),
            name = name.as_str()
        ),
        Code::SimplifyOrderBy => tr!("diagnostic-simplify-order-by"),
        Code::External(_) => diagnostic.message.clone(),
    }
}

/// The quick fix's `⌥↵` menu label in the UI language.
pub fn fix_title(fix: &Fix) -> String {
    match &fix.kind {
        FixKind::ImportClass(fqn) => tr!("fix-import-class", name = fqn.as_str()),
        FixKind::RemoveImport => tr!("fix-remove-import"),
        FixKind::RemoveAssignment => tr!("fix-remove-assignment"),
        FixKind::UseOrderByDesc => tr!("fix-use-order-by-desc"),
        FixKind::ToggleBladeEcho { to_raw: true } => tr!("fix-blade-echo-raw"),
        FixKind::ToggleBladeEcho { to_raw: false } => tr!("fix-blade-echo-escaped"),
        FixKind::RulesToArray => tr!("fix-rules-to-array"),
        FixKind::RulesToString => tr!("fix-rules-to-string"),
        FixKind::AddMethod { class, name } => {
            tr!("fix-add-method", name = name.as_str(), class = short(class))
        }
        FixKind::AddProperty { class, name } => {
            tr!(
                "fix-add-property",
                name = name.as_str(),
                class = short(class)
            )
        }
    }
}
