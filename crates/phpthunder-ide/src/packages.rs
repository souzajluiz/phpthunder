//! Dependency manifests (`composer.json`, `package.json`): which lines
//! declare a package, what is installed for it (from the lock file), and
//! its registry page. The editor shows the installed version and summary
//! at the end of each dependency line; Cmd/Ctrl+click on one opens its page.

use std::collections::HashMap;
use std::path::Path;

/// The package manager a manifest belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Manager {
    /// Composer: `composer.json`, locked in `composer.lock`, pages on Packagist.
    Composer,
    /// npm (also pnpm/Yarn manifests): `package.json`, locked in
    /// `package-lock.json`, pages on npmjs.com.
    Npm,
}

impl Manager {
    /// The manager whose manifest `path` is, by file name; `None` for any
    /// other file. Used to decide whether an open tab gets package hints.
    pub fn of_manifest(path: &Path) -> Option<Self> {
        match path.file_name()?.to_str()? {
            "composer.json" => Some(Self::Composer),
            "package.json" => Some(Self::Npm),
            _ => None,
        }
    }

    /// Manifest keys whose object lists dependencies.
    fn sections(self) -> &'static [&'static str] {
        match self {
            Self::Composer => &["require", "require-dev"],
            Self::Npm => &[
                "dependencies",
                "devDependencies",
                "peerDependencies",
                "optionalDependencies",
            ],
        }
    }

    /// The registry page of package `name`, or `None` for Composer platform
    /// requirements (`php`, `ext-*`, `lib-*`, `composer-*`) that have none.
    pub fn page_url(self, name: &str) -> Option<String> {
        match self {
            Self::Composer => name
                .contains('/')
                .then(|| format!("https://packagist.org/packages/{name}")),
            Self::Npm => Some(format!("https://www.npmjs.com/package/{name}")),
        }
    }
}

/// A dependency declared in a manifest: its 0-based line and package name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// Line of the `"name": "constraint"` entry.
    pub line: usize,
    /// Package name, as written in the key.
    pub name: String,
}

/// The dependency entries of manifest text `src`: each `"name": …` key one
/// level inside a dependency section of `manager`. Line based, so it keeps
/// working while the file is being edited and is not valid JSON; entries
/// sharing a line with their section's opening brace are not found.
pub fn dependencies(src: &str, manager: Manager) -> Vec<Dependency> {
    let mut out = Vec::new();
    // Brace depth of the open dependency section's contents, if inside one.
    let mut section: Option<usize> = None;
    let mut depth = 0usize;
    for (line, text) in src.lines().enumerate() {
        let key = quoted_key(text);
        if section == Some(depth)
            && let Some(name) = key
        {
            out.push(Dependency {
                line,
                name: name.to_string(),
            });
        }
        for b in text.bytes() {
            match b {
                b'{' => {
                    depth += 1;
                    if section.is_none() && key.is_some_and(|k| manager.sections().contains(&k)) {
                        section = Some(depth);
                    }
                }
                b'}' => {
                    if section == Some(depth) {
                        section = None;
                    }
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
        }
    }
    out
}

/// The key of a `"key": …` line, without quotes; `None` when the line does
/// not start with a quoted key followed by a colon.
fn quoted_key(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    rest[end + 1..]
        .trim_start()
        .starts_with(':')
        .then(|| &rest[..end])
}

/// What is installed for a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Locked version (`v10.48.2`, `3.4.1`).
    pub version: String,
    /// One-line summary from the package's own manifest, possibly empty.
    pub description: String,
}

/// Installed packages of a project by manager and name, read from the lock
/// files. Loaded when a project opens and again when a lock file changes.
#[derive(Debug, Default)]
pub struct Installs {
    /// Locked packages per manager, by package name.
    by_manager: HashMap<Manager, HashMap<String, Installed>>,
}

impl Installs {
    /// Reads `composer.lock` and `package-lock.json` under `root`. Missing
    /// or unreadable lock files leave that manager empty.
    pub fn load(root: &Path) -> Self {
        let mut by_manager = HashMap::new();
        if let Some(text) = read(&root.join("composer.lock")) {
            by_manager.insert(Manager::Composer, composer_lock(&text));
        }
        if let Some(text) = read(&root.join("package-lock.json")) {
            by_manager.insert(Manager::Npm, npm_lock(&text, &root.join("node_modules")));
        }
        Self { by_manager }
    }

    /// What is installed for `name` under `manager`, if locked.
    pub fn get(&self, manager: Manager, name: &str) -> Option<&Installed> {
        self.by_manager.get(&manager)?.get(name)
    }

    /// Whether `path` is a lock file `load` reads, so a change to it calls
    /// for a reload.
    pub fn is_lock_file(path: &Path) -> bool {
        path.file_name()
            .is_some_and(|n| n == "composer.lock" || n == "package-lock.json")
    }
}

/// The contents of `path`, or `None` when it cannot be read.
fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Packages of a `composer.lock`: both `packages` and `packages-dev`.
fn composer_lock(text: &str) -> HashMap<String, Installed> {
    let json: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    ["packages", "packages-dev"]
        .iter()
        .filter_map(|key| json.get(key)?.as_array())
        .flatten()
        .filter_map(|p| {
            let name = p.get("name")?.as_str()?;
            Some((
                name.to_string(),
                Installed {
                    version: str_field(p, "version"),
                    description: str_field(p, "description"),
                },
            ))
        })
        .collect()
}

/// Top-level packages of a `package-lock.json` (lockfile v2/v3
/// `packages["node_modules/<name>"]`). The lock holds no descriptions, so
/// each is read from the installed `node_modules/<name>/package.json` when
/// present.
fn npm_lock(text: &str, node_modules: &Path) -> HashMap<String, Installed> {
    let json: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let Some(packages) = json.get("packages").and_then(|p| p.as_object()) else {
        return HashMap::new();
    };
    packages
        .iter()
        .filter_map(|(key, p)| {
            let name = key.strip_prefix("node_modules/")?;
            // Nested copies (`node_modules/a/node_modules/b`) are not what
            // the manifest's own entries resolve to.
            if name.contains("/node_modules/") {
                return None;
            }
            let description = read(&node_modules.join(name).join("package.json"))
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .map(|m| str_field(&m, "description"))
                .unwrap_or_default();
            Some((
                name.to_string(),
                Installed {
                    version: str_field(p, "version"),
                    description,
                },
            ))
        })
        .collect()
}

/// String field `key` of JSON object `value`, or empty.
fn str_field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Opens `url` in the default browser. Failure to start the opener is
/// ignored: the click simply does nothing.
pub fn open_in_browser(url: &str) {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    let _ = command.arg(url).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPOSER: &str = r#"{
    "name": "acme/app",
    "require": {
        "php": "^8.2",
        "laravel/framework": "^11.0"
    },
    "require-dev": {
        "pestphp/pest": "^3.0"
    },
    "autoload": {
        "psr-4": { "App\\": "app/" }
    }
}"#;

    #[test]
    fn dependencies_should_list_entries_of_both_require_sections() {
        let names: Vec<(usize, String)> = dependencies(COMPOSER, Manager::Composer)
            .into_iter()
            .map(|d| (d.line, d.name))
            .collect();
        assert_eq!(
            names,
            [
                (3, "php".to_string()),
                (4, "laravel/framework".to_string()),
                (7, "pestphp/pest".to_string()),
            ]
        );
    }

    #[test]
    fn dependencies_should_skip_nested_objects_inside_a_section() {
        let src = "{\n  \"dependencies\": {\n    \"a\": \"1\",\n    \"b\": {\n      \"c\": \"2\"\n    }\n  }\n}";
        let names: Vec<String> = dependencies(src, Manager::Npm)
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn page_url_should_skip_composer_platform_requirements() {
        assert_eq!(Manager::Composer.page_url("ext-json"), None);
    }

    #[test]
    fn page_url_should_point_scoped_npm_packages_at_npmjs() {
        assert_eq!(
            Manager::Npm.page_url("@vue/compiler-sfc").as_deref(),
            Some("https://www.npmjs.com/package/@vue/compiler-sfc")
        );
    }

    #[test]
    fn composer_lock_should_read_version_and_description_of_dev_packages() {
        let lock = r#"{"packages":[],"packages-dev":[{"name":"pestphp/pest","version":"v3.5.1","description":"The elegant PHP Testing Framework."}]}"#;
        assert_eq!(
            composer_lock(lock).get("pestphp/pest"),
            Some(&Installed {
                version: "v3.5.1".to_string(),
                description: "The elegant PHP Testing Framework.".to_string(),
            })
        );
    }

    #[test]
    fn npm_lock_should_ignore_nested_node_modules_copies() {
        let lock = r#"{"packages":{"":{},"node_modules/vite":{"version":"5.4.0"},"node_modules/vite/node_modules/esbuild":{"version":"0.21.0"}}}"#;
        let names: Vec<String> = npm_lock(lock, Path::new("/nonexistent"))
            .into_keys()
            .collect();
        assert_eq!(names, ["vite"]);
    }
}
