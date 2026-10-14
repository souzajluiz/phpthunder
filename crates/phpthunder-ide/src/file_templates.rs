//! File templates for New from Template: built-in skeletons (PHP types,
//! tests, views, web files) plus the user's own, from
//! `~/.config/phpthunder/templates/` and the project's
//! `.phpthunder/templates/` (a project template overrides a same-named one).
//!
//! A template file is named `<Template name>.<extension>`; its text may use
//! `${NAME}`, `${NAMESPACE}`, `${NAMESPACE_LINE}` (the `namespace …;`
//! line and a blank line, or nothing), `${YEAR}`, `${DATE}`, `${USER}` and
//! `${PROJECT}`.

use std::path::Path;

/// One file template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTemplate {
    /// Name shown in the list.
    pub name: String,
    /// Extension of created files, without the dot (`php`, `blade.php`).
    pub extension: String,
    /// Template text.
    pub body: String,
}

/// Built-in templates: (name, extension, body).
const BUILTIN: &[(&str, &str, &str)] = &[
    (
        "PHP Class",
        "php",
        "<?php\n\n${NAMESPACE_LINE}class ${NAME}\n{\n}\n",
    ),
    (
        "PHP Final Class",
        "php",
        "<?php\n\ndeclare(strict_types=1);\n\n${NAMESPACE_LINE}final class ${NAME}\n{\n}\n",
    ),
    (
        "PHP Abstract Class",
        "php",
        "<?php\n\n${NAMESPACE_LINE}abstract class ${NAME}\n{\n}\n",
    ),
    (
        "PHP Interface",
        "php",
        "<?php\n\n${NAMESPACE_LINE}interface ${NAME}\n{\n}\n",
    ),
    (
        "PHP Trait",
        "php",
        "<?php\n\n${NAMESPACE_LINE}trait ${NAME}\n{\n}\n",
    ),
    (
        "PHP Enum",
        "php",
        "<?php\n\n${NAMESPACE_LINE}enum ${NAME}: string\n{\n}\n",
    ),
    ("PHP Script", "php", "<?php\n\ndeclare(strict_types=1);\n\n"),
    (
        "PHPUnit Test",
        "php",
        "<?php\n\n${NAMESPACE_LINE}use PHPUnit\\Framework\\TestCase;\n\nfinal class ${NAME} extends TestCase\n{\n    public function test_example(): void\n    {\n        $this->assertTrue(true);\n    }\n}\n",
    ),
    (
        "Pest Test",
        "php",
        "<?php\n\nit('works', function () {\n    expect(true)->toBeTrue();\n});\n",
    ),
    ("Blade View", "blade.php", "<div>\n    \n</div>\n"),
    (
        "Twig Template",
        "html.twig",
        "{% extends 'base.html.twig' %}\n\n{% block body %}\n{% endblock %}\n",
    ),
    (
        "HTML File",
        "html",
        "<!doctype html>\n<html lang=\"en\">\n<head>\n    <meta charset=\"utf-8\">\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n    <title>${NAME}</title>\n</head>\n<body>\n\n</body>\n</html>\n",
    ),
    ("CSS File", "css", ""),
    ("JavaScript File", "js", ""),
    ("TypeScript File", "ts", ""),
    ("Markdown File", "md", "# ${NAME}\n"),
    (
        "HTTP Requests",
        "http",
        "### ${NAME}\nGET http://localhost:8000/\nAccept: application/json\n",
    ),
];

/// The built-in templates.
pub fn builtin() -> Vec<FileTemplate> {
    BUILTIN
        .iter()
        .map(|(name, extension, body)| FileTemplate {
            name: (*name).to_string(),
            extension: (*extension).to_string(),
            body: (*body).to_string(),
        })
        .collect()
}

/// Templates in `dir`: each file `<name>.<extension>` (the extension being
/// everything after the first dot), sorted by name. Missing folders give
/// none.
fn from_folder(dir: &Path) -> Vec<FileTemplate> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<FileTemplate> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            let (name, extension) = file.split_once('.')?;
            let body = std::fs::read_to_string(e.path()).ok()?;
            Some(FileTemplate {
                name: name.to_string(),
                extension: extension.to_string(),
                body,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Every template for the project at `root`: the project's, then the
/// user's, then the built-in ones, a name appearing once (the first wins).
pub fn all(root: &Path) -> Vec<FileTemplate> {
    let user = std::env::var_os("HOME")
        .map(|h| Path::new(&h).join(".config/phpthunder/templates"))
        .map(|d| from_folder(&d))
        .unwrap_or_default();
    let mut out: Vec<FileTemplate> = Vec::new();
    for t in from_folder(&root.join(".phpthunder/templates"))
        .into_iter()
        .chain(user)
        .chain(builtin())
    {
        if !out.iter().any(|o| o.name == t.name) {
            out.push(t);
        }
    }
    out
}

/// Values for a template's variables.
pub struct Variables<'a> {
    /// The new file's name without extension.
    pub name: &'a str,
    /// Its namespace, from Composer's PSR-4 map.
    pub namespace: Option<&'a str>,
    /// The current year.
    pub year: i32,
    /// Today as `YYYY-MM-DD`.
    pub date: &'a str,
    /// The user's login name.
    pub user: &'a str,
    /// The project folder's name.
    pub project: &'a str,
}

/// `template`'s text with the variables filled in.
pub fn render(template: &FileTemplate, vars: &Variables) -> String {
    let namespace = vars.namespace.unwrap_or_default();
    let namespace_line = if namespace.is_empty() {
        String::new()
    } else {
        format!("namespace {namespace};\n\n")
    };
    template
        .body
        .replace("${NAMESPACE_LINE}", &namespace_line)
        .replace("${NAMESPACE}", namespace)
        .replace("${NAME}", vars.name)
        .replace("${YEAR}", &vars.year.to_string())
        .replace("${DATE}", vars.date)
        .replace("${USER}", vars.user)
        .replace("${PROJECT}", vars.project)
}

/// Today's date as `YYYY-MM-DD` in UTC.
pub fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Variables for a class `User` in `App\Models`.
    fn vars() -> Variables<'static> {
        Variables {
            name: "User",
            namespace: Some("App\\Models"),
            year: 2026,
            date: "2026-10-14",
            user: "dev",
            project: "shop",
        }
    }

    #[test]
    fn render_should_fill_the_namespace_line_and_name() {
        let class = &builtin()[0];
        assert_eq!(
            render(class, &vars()),
            "<?php\n\nnamespace App\\Models;\n\nclass User\n{\n}\n"
        );
    }

    #[test]
    fn render_without_namespace_should_drop_the_line() {
        let class = &builtin()[0];
        let out = render(
            class,
            &Variables {
                namespace: None,
                ..vars()
            },
        );
        assert_eq!(out, "<?php\n\nclass User\n{\n}\n");
    }

    #[test]
    fn project_templates_should_override_built_in_ones() {
        let root = std::env::temp_dir().join(format!("phpthunder-tpl-{}", std::process::id()));
        let dir = root.join(".phpthunder/templates");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("PHP Class.php"), "custom").unwrap();
        std::fs::write(dir.join("Action.php"), "<?php // ${NAME}").unwrap();
        let all = all(&root);
        std::fs::remove_dir_all(&root).unwrap();
        let class = all.iter().find(|t| t.name == "PHP Class").unwrap();
        assert_eq!(
            (class.body.as_str(), all.iter().any(|t| t.name == "Action")),
            ("custom", true)
        );
    }

    #[test]
    fn today_should_look_like_a_date() {
        let d = today();
        assert_eq!((d.len(), &d[4..5], &d[7..8]), (10, "-", "-"));
    }
}
