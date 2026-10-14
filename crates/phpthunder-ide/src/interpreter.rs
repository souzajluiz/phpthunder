//! Where the project's PHP runs: the local `php`, a Docker container, a
//! Docker Compose service or a server over SSH. Read from the project file
//! `.phpthunder/project.toml`; tests, quality tools, formatters, Composer
//! and run configurations all start their processes through it, with
//! project paths translated to the paths inside the container or server.
//!
//! ```toml
//! [interpreter]
//! kind = "compose"          # "local", "docker", "compose" or "ssh"
//! service = "app"           # compose service (or `container` for docker)
//! workdir = "/var/www/html" # the project folder as the interpreter sees it
//! ```

use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Project settings file, relative to the project root.
pub const PROJECT_FILE: &str = ".phpthunder/project.toml";

/// The PHP interpreter of a project.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Interpreter {
    /// A PHP binary on this machine.
    Local {
        /// The binary (`php`, or a path).
        #[serde(default = "default_php")]
        php: String,
    },
    /// A running Docker container (`docker exec`).
    Docker {
        /// Container name or id.
        container: String,
        /// The project folder inside the container.
        #[serde(default = "default_workdir")]
        workdir: String,
        /// The PHP binary inside the container.
        #[serde(default = "default_php")]
        php: String,
    },
    /// A Docker Compose service (`docker compose exec`).
    Compose {
        /// Service name.
        service: String,
        /// The project folder inside the service.
        #[serde(default = "default_workdir")]
        workdir: String,
        /// Compose file, relative to the project; Compose finds it when unset.
        #[serde(default)]
        file: Option<String>,
        /// The PHP binary inside the service.
        #[serde(default = "default_php")]
        php: String,
    },
    /// A server reached with `ssh`, holding a copy of the project.
    Ssh {
        /// `user@host` (or an alias from `~/.ssh/config`).
        host: String,
        /// The project folder on the server.
        workdir: String,
        /// The PHP binary on the server.
        #[serde(default = "default_php")]
        php: String,
    },
}

/// `php`, the default binary name.
fn default_php() -> String {
    "php".into()
}

/// `/var/www/html`, the usual project folder in PHP images.
fn default_workdir() -> String {
    "/var/www/html".into()
}

impl Default for Interpreter {
    fn default() -> Self {
        Interpreter::Local { php: default_php() }
    }
}

/// A run configuration from the project file (see `run_config`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RunConfig {
    /// Name shown in the Run menu.
    pub name: String,
    /// The command line, run through the interpreter when it starts with
    /// `php ` or a `vendor/bin` tool, as is otherwise.
    pub command: String,
}

/// Settings of one project, from `.phpthunder/project.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    /// Where PHP runs.
    pub interpreter: Interpreter,
    /// Run configurations, in menu order.
    pub run: Vec<RunConfig>,
    /// Quality tools left out of on-save analysis (`phpcs`, `psalm`, …).
    pub disabled_tools: Vec<String>,
    /// Copyright notice written at the top of files by Update Copyright;
    /// `$year`, `$project` and `$file` are filled in.
    pub copyright: Option<String>,
    /// Commands run when a matching file is saved.
    pub watcher: Vec<crate::watchers::Watcher>,
}

/// The project's settings; defaults when the file is missing or invalid
/// (an invalid file is logged).
pub fn project_config(root: &Path) -> ProjectConfig {
    let path = root.join(PROJECT_FILE);
    if !path.is_file() {
        return ProjectConfig::default();
    }
    match universal_config::ConfigLoader::load(&path) {
        Ok(config) => config,
        Err(error) => {
            crate::log::error(format_args!("{}: {error}", path.display()));
            ProjectConfig::default()
        }
    }
}

/// The project file written by "Edit run configurations" when missing:
/// every setting, commented out.
pub const PROJECT_TEMPLATE: &str = r#"# PhpThunder project settings. Commit this file to share them.

# Where PHP runs; the local `php` when this section is missing.
# [interpreter]
# kind = "compose"           # "local", "docker", "compose" or "ssh"
# service = "app"            # compose service ("container" for docker, "host" for ssh)
# workdir = "/var/www/html"  # the project folder as the interpreter sees it

# Run configurations, listed by Run (⌃⌥R).
# [[run]]
# name = "Queue worker"
# command = "php artisan queue:work"

# Quality tools not to run on save: phpstan, psalm, phpcs, php-cs-fixer, phpmd, pint.
# disabled_tools = ["phpmd"]

# Copyright notice for Update Copyright; $year, $project and $file are filled in.
# copyright = "Copyright (c) $year Acme Inc. All rights reserved."

# Commands run when a matching file is saved ($file, $dir, $name, $stem, $root).
# [[watcher]]
# name = "Sass"
# files = "*.scss"
# command = "npx sass $file $dir/$stem.css"
"#;

/// Splits a command line into words, honouring single and double quotes.
pub fn split_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut has_word = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                has_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if has_word || !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                    has_word = false;
                }
            }
            (None, c) => current.push(c),
        }
    }
    if has_word || !current.is_empty() {
        words.push(current);
    }
    words
}

/// Quotes `arg` for a POSIX shell.
fn shell_quote(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c));
    if safe {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

impl Interpreter {
    /// The interpreter configured for the project at `root`.
    pub fn of(root: &Path) -> Self {
        project_config(root).interpreter
    }

    /// Whether PHP runs on this machine.
    pub fn is_local(&self) -> bool {
        matches!(self, Interpreter::Local { .. })
    }

    /// Short description for the status bar and errors.
    pub fn label(&self) -> String {
        match self {
            Interpreter::Local { php } => php.clone(),
            Interpreter::Docker { container, .. } => format!("docker:{container}"),
            Interpreter::Compose { service, .. } => format!("compose:{service}"),
            Interpreter::Ssh { host, .. } => format!("ssh:{host}"),
        }
    }

    /// The project folder as the interpreter sees it.
    fn workdir<'a>(&'a self, root: &'a Path) -> std::borrow::Cow<'a, str> {
        match self {
            Interpreter::Local { .. } => root.to_string_lossy(),
            Interpreter::Docker { workdir, .. }
            | Interpreter::Compose { workdir, .. }
            | Interpreter::Ssh { workdir, .. } => workdir.trim_end_matches('/').into(),
        }
    }

    /// `path` (on this machine) as the interpreter sees it; paths outside
    /// the project are left alone.
    pub fn to_remote(&self, root: &Path, path: &Path) -> String {
        match path.strip_prefix(root) {
            Ok(rel) if !self.is_local() => {
                let rel = rel.to_string_lossy();
                if rel.is_empty() {
                    self.workdir(root).into_owned()
                } else {
                    format!("{}/{rel}", self.workdir(root))
                }
            }
            _ => path.to_string_lossy().into_owned(),
        }
    }

    /// A path printed by the interpreter, on this machine.
    pub fn to_local(&self, root: &Path, remote: &str) -> PathBuf {
        if self.is_local() {
            return PathBuf::from(remote);
        }
        let workdir = self.workdir(root);
        match remote.strip_prefix(workdir.as_ref()) {
            Some(rest) => root.join(rest.trim_start_matches('/')),
            None => PathBuf::from(remote),
        }
    }

    /// The program name to run: the configured PHP binary for `php`.
    fn program<'a>(&'a self, program: &'a str) -> &'a str {
        let php = match self {
            Interpreter::Local { php }
            | Interpreter::Docker { php, .. }
            | Interpreter::Compose { php, .. }
            | Interpreter::Ssh { php, .. } => php,
        };
        if program == "php" { php } else { program }
    }

    /// Arguments with project paths translated.
    fn remote_args(&self, root: &Path, args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| {
                let path = Path::new(a);
                if path.is_absolute() {
                    self.to_remote(root, path)
                } else {
                    a.to_string_lossy().into_owned()
                }
            })
            .collect()
    }

    /// A process running `program args…` (`program` is `php` or another
    /// tool such as `composer`) in the project folder, wherever the
    /// interpreter lives. Absolute project paths in `args` are translated.
    pub fn command(
        &self,
        root: &Path,
        program: &str,
        args: &[OsString],
    ) -> tokio::process::Command {
        let program = self.program(program);
        let args = self.remote_args(root, args);
        let workdir = self.workdir(root).into_owned();
        let mut cmd = match self {
            Interpreter::Local { .. } => {
                let mut c = tokio::process::Command::new(program);
                c.args(&args);
                c
            }
            Interpreter::Docker { container, .. } => {
                let mut c = tokio::process::Command::new("docker");
                c.args(["exec", "-i", "-w", &workdir, container, program])
                    .args(&args);
                c
            }
            Interpreter::Compose { service, file, .. } => {
                let mut c = tokio::process::Command::new("docker");
                c.arg("compose");
                if let Some(f) = file {
                    c.arg("-f").arg(root.join(f));
                }
                c.args(["exec", "-T", "-w", &workdir, service, program])
                    .args(&args);
                c
            }
            Interpreter::Ssh { host, .. } => {
                let mut c = tokio::process::Command::new("ssh");
                c.arg(host).arg(self.remote_line(&workdir, program, &args));
                c
            }
        };
        cmd.current_dir(root);
        cmd
    }

    /// `cd workdir && program args…` for a remote shell.
    fn remote_line(&self, workdir: &str, program: &str, args: &[String]) -> String {
        let mut line = format!("cd {} && {}", shell_quote(workdir), shell_quote(program));
        for a in args {
            line.push(' ');
            line.push_str(&shell_quote(a));
        }
        line
    }

    /// The command line to type in the terminal (which starts in the
    /// project folder) to run `program args…` through the interpreter.
    pub fn shell_line(&self, root: &Path, program: &str, args: &[String]) -> String {
        let os: Vec<OsString> = args.iter().map(OsString::from).collect();
        let program = self.program(program).to_string();
        let args = self.remote_args(root, &os);
        let quoted = |items: &[String]| {
            items
                .iter()
                .map(|a| shell_quote(a))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let workdir = self.workdir(root).into_owned();
        let line = match self {
            Interpreter::Local { .. } => {
                let mut all = vec![program];
                all.extend(args);
                quoted(&all)
            }
            Interpreter::Docker { container, .. } => {
                let mut all = vec![
                    "docker".into(),
                    "exec".into(),
                    "-it".into(),
                    "-w".into(),
                    workdir,
                    container.clone(),
                    program,
                ];
                all.extend(args);
                quoted(&all)
            }
            Interpreter::Compose { service, file, .. } => {
                let mut all = vec!["docker".to_string(), "compose".into()];
                if let Some(f) = file {
                    all.extend(["-f".to_string(), f.clone()]);
                }
                all.extend([
                    "exec".to_string(),
                    "-w".into(),
                    workdir,
                    service.clone(),
                    program,
                ]);
                all.extend(args);
                quoted(&all)
            }
            Interpreter::Ssh { host, .. } => {
                let remote = self.remote_line(&workdir, &program, &args);
                format!("ssh -t {} {}", shell_quote(host), shell_quote(&remote))
            }
        };
        line.trim().to_string()
    }

    /// The terminal command line for a run configuration's `command`: PHP,
    /// Composer, `artisan` and `vendor/bin` tools go through the
    /// interpreter; anything else runs as written.
    pub fn run_line(&self, root: &Path, command: &str) -> String {
        let words = split_words(command);
        let Some(first) = words.first() else {
            return String::new();
        };
        match first.as_str() {
            "php" | "composer" => self.shell_line(root, first, &words[1..]),
            w if w == "artisan" || w.starts_with("vendor/bin/") || w.ends_with(".php") => {
                self.shell_line(root, "php", &words)
            }
            _ => command.to_string(),
        }
    }

    /// A folder for files a tool must read or write (reports, copies to
    /// format): the system temp folder locally, else `.phpthunder/tmp` in
    /// the project, which containers see through their mount.
    pub fn scratch_dir(&self, root: &Path) -> PathBuf {
        if self.is_local() {
            return std::env::temp_dir().join(format!("phpthunder-{}", std::process::id()));
        }
        let dir = root.join(".phpthunder").join("tmp");
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(dir.join(".gitignore"), "*\n");
        }
        dir
    }

    /// Reads a file a tool wrote at `path` (a project or scratch path): from
    /// disk, or over SSH for a remote server.
    ///
    /// # Errors
    /// Returns the I/O or SSH error.
    pub async fn read_back(&self, root: &Path, path: &Path) -> Result<String, String> {
        match self {
            Interpreter::Ssh { host, .. } => {
                let remote = self.to_remote(root, path);
                let out = tokio::process::Command::new("ssh")
                    .arg(host)
                    .arg(format!("cat {}", shell_quote(&remote)))
                    .output()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(String::from_utf8_lossy(&out.stdout).into_owned())
            }
            _ => tokio::fs::read_to_string(path)
                .await
                .map_err(|e| e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compose() -> Interpreter {
        Interpreter::Compose {
            service: "app".into(),
            workdir: "/var/www/html/".into(),
            file: None,
            php: "php".into(),
        }
    }

    #[test]
    fn project_paths_should_map_into_the_container_and_back() {
        let root = Path::new("/home/me/shop");
        let i = compose();
        assert_eq!(
            i.to_remote(root, Path::new("/home/me/shop/app/User.php")),
            "/var/www/html/app/User.php"
        );
        assert_eq!(
            i.to_local(root, "/var/www/html/app/User.php"),
            PathBuf::from("/home/me/shop/app/User.php")
        );
        assert_eq!(i.to_remote(root, Path::new("/tmp/x")), "/tmp/x");
    }

    #[test]
    fn compose_shell_line_should_exec_in_the_service() {
        let root = Path::new("/home/me/shop");
        assert_eq!(
            compose().shell_line(root, "php", &["artisan".into(), "migrate".into()]),
            "docker compose exec -w /var/www/html app php artisan migrate"
        );
    }

    #[test]
    fn ssh_shell_line_should_quote_the_remote_command() {
        let i = Interpreter::Ssh {
            host: "me@box".into(),
            workdir: "/srv/shop".into(),
            php: "php8.3".into(),
        };
        assert_eq!(
            i.shell_line(Path::new("/p"), "php", &["-r".into(), "echo 1;".into()]),
            "ssh -t me@box 'cd /srv/shop && php8.3 -r '\\''echo 1;'\\'''"
        );
    }

    #[test]
    fn run_line_should_route_php_tools_through_the_interpreter() {
        let root = Path::new("/home/me/shop");
        assert_eq!(
            compose().run_line(root, "vendor/bin/pest --filter 'user can'"),
            "docker compose exec -w /var/www/html app php vendor/bin/pest --filter 'user can'"
        );
        assert_eq!(compose().run_line(root, "npm run dev"), "npm run dev");
    }

    #[test]
    fn project_file_should_parse_interpreter_and_runs() {
        let text = "[interpreter]\nkind = \"docker\"\ncontainer = \"web\"\n\n[[run]]\nname = \"Serve\"\ncommand = \"php artisan serve\"\n";
        let dir = std::env::temp_dir().join(format!("phpthunder-project-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".phpthunder")).unwrap();
        std::fs::write(dir.join(PROJECT_FILE), text).unwrap();
        let config = project_config(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            config.interpreter,
            Interpreter::Docker {
                container: "web".into(),
                workdir: "/var/www/html".into(),
                php: "php".into()
            }
        );
        assert_eq!(config.run[0].name, "Serve");
    }
}
