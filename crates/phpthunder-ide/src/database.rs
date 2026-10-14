//! Database console: SQL run against the project's database through the
//! system's command-line clients (`sqlite3`, `mysql`, `psql`), results
//! shown as a table. The connection comes from `[[database]]` in
//! `.phpthunder/project.toml`, else from the project's `.env`
//! (`DB_CONNECTION`/`DB_HOST`/… as Laravel writes them, or `DATABASE_URL`).

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Most rows shown of one result.
const MAX_ROWS: usize = 500;

/// Database engine of a connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Driver {
    /// SQLite file.
    Sqlite,
    /// MySQL or MariaDB.
    #[default]
    Mysql,
    /// PostgreSQL.
    Pgsql,
}

/// How to reach a database.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Connection {
    /// Name shown in titles.
    pub name: String,
    /// Engine.
    pub driver: Driver,
    /// Server host (MySQL, PostgreSQL).
    pub host: String,
    /// Server port; the engine's default when 0.
    pub port: u16,
    /// Database name, or the SQLite file path.
    pub database: String,
    /// User name.
    pub username: String,
    /// Password; prefer `password_env` in shared project files.
    pub password: String,
    /// Environment variable holding the password.
    pub password_env: Option<String>,
}

/// `KEY=value` pairs of a `.env` file, quotes removed.
fn dotenv(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| {
            let v = v.trim();
            let v = v
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(v);
            (k.trim().to_string(), v.to_string())
        })
        .collect()
}

/// Reads `driver://user:pass@host:port/db` (Symfony's `DATABASE_URL`).
fn from_url(url: &str) -> Option<Connection> {
    let (scheme, rest) = url.split_once("://")?;
    let driver = match scheme {
        "mysql" | "mariadb" => Driver::Mysql,
        "postgres" | "postgresql" | "pgsql" => Driver::Pgsql,
        "sqlite" => Driver::Sqlite,
        _ => return None,
    };
    if driver == Driver::Sqlite {
        return Some(Connection {
            name: "DATABASE_URL".into(),
            driver,
            database: rest.trim_start_matches('/').split('?').next()?.to_string(),
            ..Connection::default()
        });
    }
    let (auth, location) = rest.rsplit_once('@').unwrap_or(("", rest));
    let (username, password) = auth.split_once(':').unwrap_or((auth, ""));
    let (address, database) = location.split_once('/').unwrap_or((location, ""));
    let (host, port) = address.split_once(':').unwrap_or((address, "0"));
    Some(Connection {
        name: "DATABASE_URL".into(),
        driver,
        host: host.to_string(),
        port: port.parse().unwrap_or(0),
        database: database.split('?').next().unwrap_or_default().to_string(),
        username: username.to_string(),
        password: password.to_string(),
        password_env: None,
    })
}

/// The connection the project's `.env` describes.
pub fn from_dotenv(root: &Path) -> Option<Connection> {
    let env = dotenv(&std::fs::read_to_string(root.join(".env")).ok()?);
    if let Some(url) = env.get("DATABASE_URL") {
        return from_url(url);
    }
    let driver = match env.get("DB_CONNECTION")?.as_str() {
        "sqlite" => Driver::Sqlite,
        "pgsql" | "postgres" => Driver::Pgsql,
        "mysql" | "mariadb" => Driver::Mysql,
        _ => return None,
    };
    let database = env.get("DB_DATABASE").cloned().unwrap_or_else(|| {
        // Laravel's default SQLite file.
        root.join("database/database.sqlite")
            .to_string_lossy()
            .into_owned()
    });
    Some(Connection {
        name: ".env".into(),
        driver,
        host: env
            .get("DB_HOST")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".into()),
        port: env.get("DB_PORT").and_then(|p| p.parse().ok()).unwrap_or(0),
        database,
        username: env.get("DB_USERNAME").cloned().unwrap_or_default(),
        password: env.get("DB_PASSWORD").cloned().unwrap_or_default(),
        password_env: None,
    })
}

/// The project's connections: `[[database]]` entries, then the `.env` one.
pub fn connections(root: &Path) -> Vec<Connection> {
    let mut out = crate::interpreter::project_config(root).database;
    out.extend(from_dotenv(root));
    out
}

/// A command to run: program, arguments and extra environment.
pub struct Client {
    /// Program.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Environment variables (passwords never go on the command line).
    pub env: Vec<(String, String)>,
}

/// The client command for `conn`; SQL goes to its standard input. Output
/// is CSV with a header (SQLite, PostgreSQL) or tab-separated (MySQL).
pub fn client(conn: &Connection, root: &Path) -> Client {
    let password = conn
        .password_env
        .as_deref()
        .and_then(|n| std::env::var(n).ok())
        .unwrap_or_else(|| conn.password.clone());
    match conn.driver {
        Driver::Sqlite => {
            let path = Path::new(&conn.database);
            let path: PathBuf = if path.is_absolute() || conn.database == ":memory:" {
                path.to_path_buf()
            } else {
                root.join(path)
            };
            Client {
                program: "sqlite3".into(),
                args: vec![
                    "-header".into(),
                    "-csv".into(),
                    path.to_string_lossy().into_owned(),
                ],
                env: Vec::new(),
            }
        }
        Driver::Mysql => {
            let mut args = vec!["--batch".into(), "-h".into(), conn.host.clone()];
            if conn.port != 0 {
                args.extend(["-P".into(), conn.port.to_string()]);
            }
            if !conn.username.is_empty() {
                args.extend(["-u".into(), conn.username.clone()]);
            }
            args.push(conn.database.clone());
            Client {
                program: "mysql".into(),
                args,
                env: vec![("MYSQL_PWD".into(), password)],
            }
        }
        Driver::Pgsql => {
            let mut args = vec!["-X".into(), "--csv".into(), "-h".into(), conn.host.clone()];
            if conn.port != 0 {
                args.extend(["-p".into(), conn.port.to_string()]);
            }
            if !conn.username.is_empty() {
                args.extend(["-U".into(), conn.username.clone()]);
            }
            args.extend(["-d".into(), conn.database.clone(), "-f".into(), "-".into()]);
            Client {
                program: "psql".into(),
                args,
                env: vec![("PGPASSWORD".into(), password)],
            }
        }
    }
}

/// SQL listing the tables of `driver`'s database.
pub fn tables_sql(driver: Driver) -> &'static str {
    match driver {
        Driver::Sqlite => {
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name;"
        }
        Driver::Mysql => "SHOW TABLES;",
        Driver::Pgsql => {
            "SELECT tablename FROM pg_tables WHERE schemaname NOT IN ('pg_catalog', 'information_schema') ORDER BY tablename;"
        }
    }
}

/// The SQL statement around `offset`: text between `;` outside quotes and
/// comments.
pub fn statement_at(src: &str, offset: usize) -> &str {
    let b = src.as_bytes();
    let mut starts = vec![0];
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == b'\'' || c == b'"' || c == b'`' => quote = Some(c),
            None if src[i..].starts_with("--") => {
                i = src[i..].find('\n').map_or(b.len(), |n| i + n);
                continue;
            }
            None if c == b';' => starts.push(i + 1),
            None => {}
        }
        i += 1;
    }
    let at = starts.partition_point(|&s| s <= offset).saturating_sub(1);
    let start = starts[at];
    let end = starts.get(at + 1).map_or(src.len(), |&e| e);
    let statement = src[start..end].trim();
    if statement.trim_end_matches(';').trim().is_empty() && at > 0 {
        // Caret just after a `;`: the statement before it.
        return src[starts[at - 1]..start].trim();
    }
    statement
}

/// Splits one CSV line (double-quoted fields) or, with `tab`, one
/// tab-separated line.
fn fields(line: &str, tab: bool) -> Vec<String> {
    if tab {
        return line.split('\t').map(str::to_string).collect();
    }
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => out.push(std::mem::take(&mut field)),
            (c, _) => field.push(c),
        }
    }
    out.push(field);
    out
}

/// The client's output as rows: header first. MySQL prints tab-separated
/// values, the others CSV.
pub fn rows(output: &str, driver: Driver) -> Vec<Vec<String>> {
    output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| fields(l, driver == Driver::Mysql))
        .collect()
}

/// `rows` (header first) as a Markdown table, at most [`MAX_ROWS`] rows.
pub fn markdown_table(rows: &[Vec<String>], more: &str) -> String {
    let Some((header, body)) = rows.split_first() else {
        return String::new();
    };
    let cell = |s: &str| s.replace('|', "\\|").replace('\n', " ");
    let mut out = format!(
        "| {} |\n|{}|\n",
        header
            .iter()
            .map(|h| cell(h))
            .collect::<Vec<_>>()
            .join(" | "),
        header.iter().map(|_| "---").collect::<Vec<_>>().join("|")
    );
    for row in body.iter().take(MAX_ROWS) {
        out.push_str(&format!(
            "| {} |\n",
            row.iter().map(|c| cell(c)).collect::<Vec<_>>().join(" | ")
        ));
    }
    if body.len() > MAX_ROWS {
        out.push_str(&format!("\n{more}\n"));
    }
    out
}

/// Runs `sql` on `conn` and returns the client's output.
///
/// # Errors
/// Returns the client's error (missing tool, access denied, SQL error).
pub async fn run(conn: Connection, root: PathBuf, sql: String) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    let client = client(&conn, &root);
    let mut child = tokio::process::Command::new(&client.program)
        .args(&client.args)
        .envs(client.env)
        .current_dir(&root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", client.program))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(sql.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().await.map_err(|e| e.to_string())?;
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() || (conn.driver == Driver::Sqlite && !stderr.is_empty()) {
        return Err(if stderr.is_empty() {
            format!("{} failed", client.program)
        } else {
            stderr
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_at_should_pick_the_statement_of_the_caret() {
        let src = "SELECT 1;\n-- a; comment\nSELECT ';' AS x;\n";
        let at = src.find("AS").unwrap();
        assert_eq!(statement_at(src, at), "-- a; comment\nSELECT ';' AS x;");
        assert_eq!(statement_at(src, 3), "SELECT 1;");
    }

    #[test]
    fn database_urls_should_give_connections() {
        let c = from_url("postgresql://app:secret@db:5432/shop?serverVersion=16").unwrap();
        assert_eq!(
            (
                c.driver,
                c.host.as_str(),
                c.port,
                c.database.as_str(),
                c.username.as_str(),
                c.password.as_str()
            ),
            (Driver::Pgsql, "db", 5432, "shop", "app", "secret")
        );
    }

    #[test]
    fn dotenv_should_read_laravel_keys() {
        let env = dotenv("DB_CONNECTION=mysql\nDB_PASSWORD=\"p w\"\n# DB_HOST=x\n");
        assert_eq!(
            (
                env["DB_CONNECTION"].as_str(),
                env["DB_PASSWORD"].as_str(),
                env.contains_key("# DB_HOST")
            ),
            ("mysql", "p w", false)
        );
    }

    #[test]
    fn csv_and_tsv_should_become_tables() {
        let csv = rows("id,name\n1,\"Ana, B\"\n", Driver::Sqlite);
        assert_eq!(csv[1], ["1", "Ana, B"]);
        let table = markdown_table(&rows("id\tname\n1\tA|B\n", Driver::Mysql), "");
        assert_eq!(table, "| id | name |\n|---|---|\n| 1 | A\\|B |\n");
    }

    #[test]
    fn passwords_should_stay_off_the_command_line() {
        let conn = Connection {
            driver: Driver::Mysql,
            host: "127.0.0.1".into(),
            database: "app".into(),
            username: "root".into(),
            password: "secret".into(),
            ..Connection::default()
        };
        let c = client(&conn, Path::new("/p"));
        assert!(!c.args.iter().any(|a| a.contains("secret")));
        assert_eq!(c.env, [("MYSQL_PWD".to_string(), "secret".to_string())]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sqlite_should_run_when_installed() {
        if std::process::Command::new("sqlite3")
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        let conn = Connection {
            driver: Driver::Sqlite,
            database: ":memory:".into(),
            ..Connection::default()
        };
        let out = run(
            conn,
            std::env::temp_dir(),
            "SELECT 1 AS one, 'a' AS two;".into(),
        )
        .await
        .unwrap();
        assert_eq!(
            rows(&out, Driver::Sqlite),
            [vec!["one", "two"], vec!["1", "a"]]
        );
    }
}
