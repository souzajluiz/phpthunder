//! Deployment: `[[deploy]]` servers of `.phpthunder/project.toml`, the
//! project or single files uploaded to them, optionally on every save.
//! Transfers use the system's tools: `rsync` over SSH, `curl` for SFTP,
//! FTP(S) and WebDAV files, `lftp` to mirror a whole project over FTP or
//! SFTP.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// How files reach a server.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// `rsync` over SSH.
    #[default]
    Rsync,
    /// SFTP.
    Sftp,
    /// FTP or FTPS.
    Ftp,
    /// WebDAV over HTTP(S).
    Webdav,
}

/// One `[[deploy]]` server.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Server {
    /// Name shown in the list.
    pub name: String,
    /// Transfer protocol.
    #[serde(default)]
    pub protocol: Protocol,
    /// `user@host` for rsync and SFTP; the base URL for FTP and WebDAV
    /// (`ftp://host/www`, `https://dav.example.com/site`).
    pub host: String,
    /// Remote folder of the project (rsync, SFTP).
    #[serde(default)]
    pub path: String,
    /// Paths left out of project uploads.
    #[serde(default = "default_excludes")]
    pub exclude: Vec<String>,
    /// Upload each saved file.
    #[serde(default)]
    pub upload_on_save: bool,
    /// Environment variable holding `user:password` for FTP, SFTP and
    /// WebDAV, so no password sits in the project file.
    #[serde(default)]
    pub credentials_env: Option<String>,
}

/// Paths project uploads skip unless the server lists its own.
fn default_excludes() -> Vec<String> {
    [".git", ".phpthunder", "node_modules", ".idea", ".env"]
        .map(String::from)
        .to_vec()
}

/// A program and its arguments.
pub type CommandLine = (String, Vec<String>);

/// `rel` (with `/`) of `file` inside `root`.
fn relative(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The remote URL of `rel` for curl-based protocols.
fn url(server: &Server, rel: &str) -> String {
    match server.protocol {
        Protocol::Sftp => {
            let folder = server.path.trim_matches('/');
            if folder.is_empty() {
                format!("sftp://{}/{rel}", server.host)
            } else {
                format!("sftp://{}/{folder}/{rel}", server.host)
            }
        }
        _ => format!("{}/{rel}", server.host.trim_end_matches('/')),
    }
}

/// `--user` arguments for curl from the server's credentials variable.
fn credentials(server: &Server) -> Vec<String> {
    server
        .credentials_env
        .as_deref()
        .and_then(|name| std::env::var(name).ok())
        .map(|value| vec!["--user".to_string(), value])
        .unwrap_or_default()
}

/// The command uploading `file` (inside `root`) to `server`.
pub fn upload_file(server: &Server, root: &Path, file: &Path) -> CommandLine {
    let rel = relative(root, file);
    match server.protocol {
        Protocol::Rsync => (
            "rsync".into(),
            vec![
                "-az".into(),
                "--relative".into(),
                format!("./{rel}"),
                format!("{}:{}/", server.host, server.path.trim_end_matches('/')),
            ],
        ),
        Protocol::Sftp | Protocol::Ftp | Protocol::Webdav => {
            let mut args = vec!["-sS".into(), "--fail".into()];
            if server.protocol != Protocol::Webdav {
                args.push("--ftp-create-dirs".into());
            }
            args.extend(credentials(server));
            args.push("-T".into());
            args.push(file.to_string_lossy().into_owned());
            args.push(url(server, &rel));
            ("curl".into(), args)
        }
    }
}

/// The command uploading the whole project to `server`: rsync, or lftp's
/// reverse mirror for FTP and SFTP; `None` for WebDAV (files only).
pub fn upload_project(server: &Server, root: &Path) -> Option<CommandLine> {
    match server.protocol {
        Protocol::Rsync => {
            let mut args = vec!["-az".to_string()];
            for e in &server.exclude {
                args.push(format!("--exclude={e}"));
            }
            args.push(format!("{}/", root.to_string_lossy().trim_end_matches('/')));
            args.push(format!(
                "{}:{}/",
                server.host,
                server.path.trim_end_matches('/')
            ));
            Some(("rsync".into(), args))
        }
        Protocol::Ftp | Protocol::Sftp => {
            let target = match server.protocol {
                Protocol::Sftp => format!("sftp://{}", server.host),
                _ => server.host.clone(),
            };
            let remote = if server.protocol == Protocol::Sftp {
                server.path.clone()
            } else {
                String::new()
            };
            let excludes: String = server
                .exclude
                .iter()
                .map(|e| format!(" --exclude-glob {e}"))
                .collect();
            let login = server
                .credentials_env
                .as_deref()
                .and_then(|n| std::env::var(n).ok())
                .map(|c| format!("set ftp:ssl-allow yes; open -u {c} {target}; "))
                .unwrap_or_else(|| format!("open {target}; "));
            let script = format!(
                "{login}mirror -R --verbose=0{excludes} {} {remote}; quit",
                root.to_string_lossy()
            );
            Some(("lftp".into(), vec!["-c".into(), script]))
        }
        Protocol::Webdav => None,
    }
}

/// Runs `command` in `root`.
///
/// # Errors
/// Returns the tool's last error line, or why it could not start.
pub async fn run(root: PathBuf, command: CommandLine) -> Result<(), String> {
    let (program, args) = command;
    let out = tokio::process::Command::new(&program)
        .args(&args)
        .current_dir(&root)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(format!(
        "{program}: {}",
        stderr
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("failed")
            .trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server of `protocol` on `host`.
    fn server(protocol: Protocol, host: &str) -> Server {
        Server {
            name: "staging".into(),
            protocol,
            host: host.into(),
            path: "/var/www/app".into(),
            exclude: default_excludes(),
            upload_on_save: false,
            credentials_env: None,
        }
    }

    #[test]
    fn rsync_should_upload_files_with_their_folders() {
        let (program, args) = upload_file(
            &server(Protocol::Rsync, "deploy@web"),
            Path::new("/p"),
            Path::new("/p/app/User.php"),
        );
        assert_eq!(
            (program.as_str(), args.as_slice()),
            (
                "rsync",
                [
                    "-az",
                    "--relative",
                    "./app/User.php",
                    "deploy@web:/var/www/app/"
                ]
                .map(String::from)
                .as_slice()
            )
        );
    }

    #[test]
    fn curl_should_put_files_under_the_base_url() {
        let (_, args) = upload_file(
            &server(Protocol::Webdav, "https://dav.example.test/site/"),
            Path::new("/p"),
            Path::new("/p/index.php"),
        );
        assert_eq!(
            args.last().unwrap(),
            "https://dav.example.test/site/index.php"
        );
        let (_, args) = upload_file(
            &server(Protocol::Sftp, "me@host"),
            Path::new("/p"),
            Path::new("/p/a/b.php"),
        );
        assert_eq!(args.last().unwrap(), "sftp://me@host/var/www/app/a/b.php");
    }

    #[test]
    fn project_uploads_should_skip_excluded_paths() {
        let (_, args) =
            upload_project(&server(Protocol::Rsync, "deploy@web"), Path::new("/p")).unwrap();
        assert!(args.contains(&"--exclude=node_modules".to_string()));
        assert!(upload_project(&server(Protocol::Webdav, "https://x"), Path::new("/p")).is_none());
    }
}
