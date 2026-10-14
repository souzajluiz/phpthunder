//! File types: what kind of file a path holds, decided once when the file
//! opens. The editor picks its highlighter from it, the status bar shows
//! its name, and PHP-only features (inspections, completion, refactorings)
//! check it.
//!
//! Lookup order, first hit wins:
//! 1. exact file name (`composer.lock`, `Dockerfile`, `.env`, `artisan`);
//! 2. name patterns, longest first (`*.blade.php`, `.env.*`, `Dockerfile.*`);
//! 3. extension (`php`, `json`, `yml`, …), case-insensitive;
//! 4. the shebang of a file without extension (`#!/usr/bin/env php`);
//! 5. plain text.

use std::path::Path;

/// The kind of a file, as far as the editor cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileType {
    /// PHP source.
    Php,
    /// Blade template (`*.blade.php`).
    Blade,
    /// HTML.
    Html,
    /// XML, including `phpunit.xml(.dist)`, SVG and XLIFF.
    Xml,
    /// Vue single-file component.
    Vue,
    /// JavaScript (`js`, `mjs`, `cjs`, `jsx`).
    JavaScript,
    /// TypeScript (`ts`, `mts`, `cts`, `tsx`).
    TypeScript,
    /// JSON and JSON with comments (`composer.lock`, `tsconfig.json`).
    Json,
    /// YAML.
    Yaml,
    /// TOML.
    Toml,
    /// CSS.
    Css,
    /// SCSS, Sass and Less.
    Scss,
    /// Markdown.
    Markdown,
    /// SQL.
    Sql,
    /// Shell script.
    Shell,
    /// Dockerfile.
    Dockerfile,
    /// `.env` files.
    DotEnv,
    /// INI-style configuration (`php.ini`, `.editorconfig`, `.gitattributes`).
    Ini,
    /// Ignore lists (`.gitignore`, `.dockerignore`, …).
    Ignore,
    /// Makefile.
    Makefile,
    /// Comma- or tab-separated values.
    Csv,
    /// Raster or vector image shown as a picture, not text (`png`, `jpg`, …).
    Image,
    /// Other binary content: fonts, audio, video, archives, databases.
    Binary,
    /// Plain text, and anything not recognized.
    Text,
}

/// Exact file names, case-insensitive.
const NAMES: &[(&str, FileType)] = &[
    ("artisan", FileType::Php),
    ("composer.lock", FileType::Json),
    (".babelrc", FileType::Json),
    (".eslintrc", FileType::Json),
    (".prettierrc", FileType::Json),
    ("dockerfile", FileType::Dockerfile),
    ("containerfile", FileType::Dockerfile),
    (".env", FileType::DotEnv),
    ("makefile", FileType::Makefile),
    ("gnumakefile", FileType::Makefile),
    (".editorconfig", FileType::Ini),
    (".gitattributes", FileType::Ini),
    (".gitconfig", FileType::Ini),
    (".gitmodules", FileType::Ini),
    (".htaccess", FileType::Ini),
    (".npmrc", FileType::Ini),
    ("license", FileType::Text),
    (".ds_store", FileType::Binary),
    ("readme", FileType::Markdown),
];

/// Name patterns: a prefix or a suffix of the lowercased name, tried
/// longest first so `*.blade.php` wins over `*.php`.
const PATTERNS: &[(&str, FileType)] = &[
    ("*.blade.php", FileType::Blade),
    // Class diagrams drawn by the IDE: shown, not edited as XML.
    ("*.diagram.svg", FileType::Image),
    ("*.xml.dist", FileType::Xml),
    ("*.neon.dist", FileType::Yaml),
    ("*.yml.dist", FileType::Yaml),
    ("*.yaml.dist", FileType::Yaml),
    ("*.php.dist", FileType::Php),
    ("*.env.example", FileType::DotEnv),
    (".env.*", FileType::DotEnv),
    ("dockerfile.*", FileType::Dockerfile),
    ("*.dockerfile", FileType::Dockerfile),
    ("*ignore", FileType::Ignore),
    ("docker-compose*.yml", FileType::Yaml),
];

/// Extensions, lowercase.
const EXTENSIONS: &[(&str, FileType)] = &[
    ("php", FileType::Php),
    ("phtml", FileType::Php),
    // Laravel `make:*` stubs: PHP with `{{ placeholder }}`s.
    ("stub", FileType::Php),
    ("inc", FileType::Php),
    ("html", FileType::Html),
    ("htm", FileType::Html),
    ("xml", FileType::Xml),
    ("xsd", FileType::Xml),
    ("xsl", FileType::Xml),
    ("svg", FileType::Xml),
    ("xlf", FileType::Xml),
    ("xliff", FileType::Xml),
    ("vue", FileType::Vue),
    ("js", FileType::JavaScript),
    ("mjs", FileType::JavaScript),
    ("cjs", FileType::JavaScript),
    ("jsx", FileType::JavaScript),
    ("ts", FileType::TypeScript),
    ("mts", FileType::TypeScript),
    ("cts", FileType::TypeScript),
    ("tsx", FileType::TypeScript),
    ("json", FileType::Json),
    ("jsonc", FileType::Json),
    ("json5", FileType::Json),
    ("jsonl", FileType::Json),
    ("webmanifest", FileType::Json),
    ("map", FileType::Json),
    ("yml", FileType::Yaml),
    ("yaml", FileType::Yaml),
    ("neon", FileType::Yaml),
    ("toml", FileType::Toml),
    ("css", FileType::Css),
    ("scss", FileType::Scss),
    ("sass", FileType::Scss),
    ("less", FileType::Scss),
    ("md", FileType::Markdown),
    ("markdown", FileType::Markdown),
    ("sql", FileType::Sql),
    ("sh", FileType::Shell),
    ("bash", FileType::Shell),
    ("zsh", FileType::Shell),
    ("ini", FileType::Ini),
    ("cfg", FileType::Ini),
    ("conf", FileType::Ini),
    ("properties", FileType::Ini),
    ("env", FileType::DotEnv),
    ("mk", FileType::Makefile),
    ("csv", FileType::Csv),
    ("tsv", FileType::Csv),
    ("txt", FileType::Text),
    ("log", FileType::Text),
    ("png", FileType::Image),
    ("jpg", FileType::Image),
    ("jpeg", FileType::Image),
    ("gif", FileType::Image),
    ("webp", FileType::Image),
    ("avif", FileType::Image),
    ("bmp", FileType::Image),
    ("ico", FileType::Image),
    ("ttf", FileType::Binary),
    ("otf", FileType::Binary),
    ("woff", FileType::Binary),
    ("woff2", FileType::Binary),
    ("eot", FileType::Binary),
    ("mp3", FileType::Binary),
    ("wav", FileType::Binary),
    ("ogg", FileType::Binary),
    ("mp4", FileType::Binary),
    ("webm", FileType::Binary),
    ("pdf", FileType::Binary),
    ("zip", FileType::Binary),
    ("gz", FileType::Binary),
    ("tgz", FileType::Binary),
    ("tar", FileType::Binary),
    ("phar", FileType::Binary),
    ("sqlite", FileType::Binary),
    ("db", FileType::Binary),
    ("so", FileType::Binary),
    ("dylib", FileType::Binary),
    ("exe", FileType::Binary),
];

/// Shebang interpreters for files without an extension.
const SHEBANGS: &[(&str, FileType)] = &[
    ("php", FileType::Php),
    ("bash", FileType::Shell),
    ("sh", FileType::Shell),
    ("zsh", FileType::Shell),
    ("node", FileType::JavaScript),
];

impl FileType {
    /// Detects the type of the file at `path`; `first_line` is the file's
    /// first line when known, for the shebang of files without extension.
    pub fn detect(path: &Path, first_line: Option<&str>) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if let Some((_, t)) = NAMES.iter().find(|(n, _)| *n == name) {
            return *t;
        }
        let mut patterns: Vec<&(&str, FileType)> = PATTERNS.iter().collect();
        patterns.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
        if let Some((_, t)) = patterns.iter().find(|(p, _)| glob(p, &name)) {
            return *t;
        }
        let ext = name.rsplit_once('.').map(|(_, e)| e);
        if let Some(ext) = ext.filter(|_| !name.starts_with('.') || name.matches('.').count() > 1)
            && let Some((_, t)) = EXTENSIONS.iter().find(|(e, _)| *e == ext)
        {
            return *t;
        }
        if ext.is_none()
            && let Some(t) = first_line.and_then(shebang)
        {
            return t;
        }
        FileType::Text
    }

    /// Display name for the status bar.
    pub fn name(self) -> &'static str {
        match self {
            FileType::Php => "PHP",
            FileType::Blade => "Blade",
            FileType::Html => "HTML",
            FileType::Xml => "XML",
            FileType::Vue => "Vue",
            FileType::JavaScript => "JavaScript",
            FileType::TypeScript => "TypeScript",
            FileType::Json => "JSON",
            FileType::Yaml => "YAML",
            FileType::Toml => "TOML",
            FileType::Css => "CSS",
            FileType::Scss => "SCSS",
            FileType::Markdown => "Markdown",
            FileType::Sql => "SQL",
            FileType::Shell => "Shell",
            FileType::Dockerfile => "Dockerfile",
            FileType::DotEnv => ".env",
            FileType::Ini => "INI",
            FileType::Ignore => "Ignore",
            FileType::Makefile => "Makefile",
            FileType::Csv => "CSV",
            FileType::Image => "Image",
            FileType::Binary => "Binary",
            FileType::Text => "Text",
        }
    }
}

/// Matches a lowercase file name against a pattern with one `*`.
fn glob(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        Some((head, tail)) => {
            name.len() >= head.len() + tail.len() && name.starts_with(head) && name.ends_with(tail)
        }
        None => pattern == name,
    }
}

/// File type named by a `#!` line (`#!/usr/bin/env php`, `#!/bin/bash`).
fn shebang(line: &str) -> Option<FileType> {
    let rest = line.strip_prefix("#!")?;
    let mut words = rest.split_whitespace();
    let program = words.next()?.rsplit('/').next()?;
    let interpreter = if program == "env" {
        words.find(|w| !w.starts_with('-'))?
    } else {
        program
    };
    // `php8.3`, `python3` -> `php`, `python`
    let base = interpreter.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    SHEBANGS.iter().find(|(s, _)| *s == base).map(|(_, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(name: &str) -> FileType {
        FileType::detect(Path::new(name), None)
    }

    #[test]
    fn detect_should_prefer_names_then_patterns_then_extensions() {
        assert_eq!(detect("app/User.php"), FileType::Php);
        assert_eq!(detect("resources/views/welcome.blade.php"), FileType::Blade);
        assert_eq!(detect("composer.lock"), FileType::Json);
        assert_eq!(detect("composer.json"), FileType::Json);
        assert_eq!(detect("phpunit.xml.dist"), FileType::Xml);
        assert_eq!(detect("phpstan.neon.dist"), FileType::Yaml);
        assert_eq!(detect(".env"), FileType::DotEnv);
        assert_eq!(detect(".env.testing"), FileType::DotEnv);
        assert_eq!(detect("Dockerfile"), FileType::Dockerfile);
        assert_eq!(detect("Dockerfile.prod"), FileType::Dockerfile);
        assert_eq!(detect("docker-compose.override.yml"), FileType::Yaml);
        assert_eq!(detect(".gitignore"), FileType::Ignore);
        assert_eq!(detect(".editorconfig"), FileType::Ini);
        assert_eq!(detect("vite.config.ts"), FileType::TypeScript);
        assert_eq!(detect("App.VUE"), FileType::Vue);
        assert_eq!(detect("README.md"), FileType::Markdown);
        assert_eq!(detect("notes"), FileType::Text);
        assert_eq!(detect("stubs/model.stub"), FileType::Php);
        assert_eq!(detect("public/logo.PNG"), FileType::Image);
        assert_eq!(detect("fonts/Geist.ttf"), FileType::Binary);
        assert_eq!(detect(".DS_Store"), FileType::Binary);
    }

    #[test]
    fn detect_should_read_the_shebang_of_files_without_extension() {
        assert_eq!(
            FileType::detect(Path::new("bin/console"), Some("#!/usr/bin/env php")),
            FileType::Php
        );
        assert_eq!(
            FileType::detect(Path::new("deploy"), Some("#!/bin/bash -e")),
            FileType::Shell
        );
        assert_eq!(
            FileType::detect(Path::new("run"), Some("#!/usr/bin/env -S php8.3 -d x")),
            FileType::Php
        );
        // An extension wins over the shebang.
        assert_eq!(
            FileType::detect(Path::new("x.txt"), Some("#!/bin/sh")),
            FileType::Text
        );
    }
}
