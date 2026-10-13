//! Supported UI languages and picking one for this run.

/// A UI language and its Fluent catalogs (embedded at build time).
pub struct Locale {
    /// BCP 47 identifier, e.g. `pt-BR`.
    pub id: &'static str,
    /// Name of the language in itself, for a future language picker.
    /// Fluent sources, one per UI area.
    pub sources: &'static [&'static str],
}

/// Embeds the catalogs of one locale directory, one file per UI area.
///
/// CONTEXT: every locale must ship the same files; adding a language means
/// adding a `locales/<id>/` folder and one entry in [`LOCALES`].
macro_rules! catalogs {
    ($dir:literal) => {
        &[
            include_str!(concat!("../../locales/", $dir, "/workbench.ftl")),
            include_str!(concat!("../../locales/", $dir, "/panels.ftl")),
            include_str!(concat!("../../locales/", $dir, "/app.ftl")),
            include_str!(concat!("../../locales/", $dir, "/inspections.ftl")),
            include_str!(concat!("../../locales/", $dir, "/templates.ftl")),
        ]
    };
}

/// English: the source language and the fallback for missing messages.
pub const ENGLISH: Locale = Locale {
    id: "en-US",
    sources: catalogs!("en-US"),
};

/// Every supported UI language; the first is the fallback.
pub const LOCALES: &[Locale] = &[
    ENGLISH,
    Locale {
        id: "pt-BR",
        sources: catalogs!("pt-BR"),
    },
];

/// Picks the UI language: `PHPTHUNDER_LANG` if set (e.g. `pt-BR`, `en`),
/// else the operating system's language, else English. A language without
/// an exact match falls back to the first locale with the same base
/// language (`pt-PT` -> `pt-BR`).
pub fn detect_locale() -> &'static Locale {
    let requested = std::env::var("PHPTHUNDER_LANG")
        .ok()
        .or_else(sys_locale::get_locale)
        .unwrap_or_default();
    match_locale(&requested)
}

/// The supported locale best matching a language tag like `pt_BR.UTF-8`.
pub fn match_locale(tag: &str) -> &'static Locale {
    let tag = tag.split('.').next().unwrap_or("").replace('_', "-");
    let base = tag.split('-').next().unwrap_or("").to_ascii_lowercase();
    LOCALES
        .iter()
        .find(|l| l.id.eq_ignore_ascii_case(&tag))
        .or_else(|| {
            LOCALES.iter().find(|l| {
                l.id.split('-')
                    .next()
                    .is_some_and(|b| b.eq_ignore_ascii_case(&base))
            })
        })
        .unwrap_or(&LOCALES[0])
}
