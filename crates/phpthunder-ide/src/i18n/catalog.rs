//! Looking up translated UI strings.

use super::*;
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use std::sync::OnceLock;
use unic_langid::LanguageIdentifier;

/// Messages of the active language, with English behind them.
struct Catalog {
    /// The chosen language.
    active: FluentBundle<FluentResource>,
    /// English, for messages the active language lacks.
    fallback: FluentBundle<FluentResource>,
}

/// The catalog for this run, set once by [`init_locale`].
static CATALOG: OnceLock<Catalog> = OnceLock::new();

/// Builds a bundle from a locale's sources.
///
/// # Panics
/// Panics when an embedded catalog does not parse; the catalog tests catch
/// that before a build ships.
fn bundle(locale: &Locale) -> FluentBundle<FluentResource> {
    let id: LanguageIdentifier = locale.id.parse().unwrap_or_default();
    let mut bundle = FluentBundle::new_concurrent(vec![id]);
    // Unicode isolation marks around arguments would show up as boxes in
    // the editor's fonts.
    bundle.set_use_isolating(false);
    for source in locale.sources {
        let resource = FluentResource::try_new((*source).to_string())
            .unwrap_or_else(|(_, errors)| panic!("invalid {} catalog: {errors:?}", locale.id));
        // A duplicate key is a catalog bug; the last definition wins.
        let _ = bundle.add_resource(resource);
    }
    bundle
}

/// Sets the UI language for the rest of the run. Later calls are ignored.
pub fn init_locale(locale: &'static Locale) {
    let _ = CATALOG.set(Catalog {
        active: bundle(locale),
        fallback: bundle(&ENGLISH),
    });
}

/// The catalog, initialised with the detected language on first use.
fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(|| {
        let locale = detect_locale();
        Catalog {
            active: bundle(locale),
            fallback: bundle(&ENGLISH),
        }
    })
}

/// Formats message `id` with optional arguments in the active language,
/// falling back to English, then to the id itself (so a missing message is
/// visible but harmless). Use the [`tr!`](crate::tr) macro.
pub fn translate(id: &str, args: Option<&FluentArgs>) -> String {
    let catalog = catalog();
    for bundle in [&catalog.active, &catalog.fallback] {
        if let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) {
            let mut errors = Vec::new();
            return bundle
                .format_pattern(pattern, args, &mut errors)
                .into_owned();
        }
    }
    id.to_string()
}

/// Translates a UI string: `tr!("status-indexing")` or, with arguments,
/// `tr!("usages-title", name = symbol, count = rows.len())`.
#[macro_export]
macro_rules! tr {
    ($id:literal) => {
        $crate::i18n::translate($id, None)
    };
    ($id:literal, $($key:ident = $value:expr),+ $(,)?) => {{
        let mut args = fluent_bundle::FluentArgs::new();
        $(args.set(stringify!($key), $value);)+
        $crate::i18n::translate($id, Some(&args))
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Formats `id` from `locale`'s bundle alone (no fallback).
    fn format(locale: &str, id: &str, args: &FluentArgs) -> String {
        let bundle = bundle(match_locale(locale));
        let pattern = bundle
            .get_message(id)
            .and_then(|m| m.value())
            .expect("message exists");
        bundle
            .format_pattern(pattern, Some(args), &mut Vec::new())
            .into_owned()
    }

    #[test]
    fn plurals_should_follow_each_language() {
        let mut one = FluentArgs::new();
        one.set("count", 1);
        let mut many = FluentArgs::new();
        many.set("count", 3);
        assert_eq!(
            format("en-US", "notice-replaced", &one),
            "Replaced 1 occurrence"
        );
        assert_eq!(
            format("en-US", "notice-replaced", &many),
            "Replaced 3 occurrences"
        );
        assert_eq!(
            format("pt-BR", "notice-replaced", &many),
            "3 ocorrências substituídas"
        );
    }

    #[test]
    fn translate_should_return_the_id_for_unknown_messages() {
        assert_eq!(translate("no-such-message", None), "no-such-message");
    }
}
