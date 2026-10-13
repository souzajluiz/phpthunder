//! UI translations: Fluent catalogs per language in `locales/<id>/`, the
//! language picked from the system (or `PHPTHUNDER_LANG`), and the `tr!`
//! macro. English is the source language and the fallback.

mod catalog;
mod inspection;
mod locale;

pub use catalog::*;
pub use inspection::*;
pub use locale::*;

#[cfg(test)]
mod tests {
    use super::*;
    use fluent_bundle::FluentResource;
    use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};
    use std::collections::{BTreeMap, BTreeSet};

    /// Collects `$variable` names used anywhere in a pattern.
    fn variables(pattern: &Pattern<&str>, out: &mut BTreeSet<String>) {
        for element in &pattern.elements {
            if let PatternElement::Placeable { expression } = element {
                expression_variables(expression, out);
            }
        }
    }

    /// Collects variables of one expression, including select variants.
    fn expression_variables(expression: &Expression<&str>, out: &mut BTreeSet<String>) {
        match expression {
            Expression::Inline(InlineExpression::VariableReference { id }) => {
                out.insert(id.name.to_string());
            }
            Expression::Select { selector, variants } => {
                if let InlineExpression::VariableReference { id } = selector {
                    out.insert(id.name.to_string());
                }
                for v in variants {
                    variables(&v.value, out);
                }
            }
            _ => {}
        }
    }

    /// Message id -> variables, for every message of a locale.
    fn messages(locale: &Locale) -> BTreeMap<String, BTreeSet<String>> {
        let mut out = BTreeMap::new();
        for source in locale.sources {
            let resource = FluentResource::try_new((*source).to_string())
                .unwrap_or_else(|(_, e)| panic!("{} does not parse: {e:?}", locale.id));
            for entry in resource.entries() {
                if let Entry::Message(m) = entry {
                    let mut vars = BTreeSet::new();
                    if let Some(value) = &m.value {
                        variables(value, &mut vars);
                    }
                    assert!(
                        out.insert(m.id.name.to_string(), vars).is_none(),
                        "{}: duplicate key {}",
                        locale.id,
                        m.id.name
                    );
                }
            }
        }
        out
    }

    #[test]
    fn every_locale_should_translate_every_english_message_with_the_same_arguments() {
        let english = messages(&ENGLISH);
        for locale in &LOCALES[1..] {
            let translated = messages(locale);
            for (id, vars) in &english {
                let other = translated
                    .get(id)
                    .unwrap_or_else(|| panic!("{} lacks `{id}`", locale.id));
                assert_eq!(
                    other, vars,
                    "{}: `{id}` uses different arguments",
                    locale.id
                );
            }
            for id in translated.keys() {
                assert!(
                    english.contains_key(id),
                    "{} has `{id}`, which English does not",
                    locale.id
                );
            }
        }
    }

    #[test]
    fn match_locale_should_fall_back_by_base_language() {
        assert_eq!(match_locale("pt_BR.UTF-8").id, "pt-BR");
        assert_eq!(match_locale("pt-PT").id, "pt-BR");
        assert_eq!(match_locale("de-DE").id, "en-US");
        assert_eq!(match_locale("").id, "en-US");
    }
}
