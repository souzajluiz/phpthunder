//! The SQL console: run the statement at the caret (or the selection) on
//! the project's database and see the rows in a tab; list the tables and
//! open one.

use super::*;
use crate::database::{self, Connection};
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;

impl App {
    /// The connection SQL runs on: the first configured, else `.env`'s.
    fn connection(&mut self) -> Option<(Connection, std::path::PathBuf)> {
        let root = self.tree.root.clone()?;
        match database::connections(&root).into_iter().next() {
            Some(conn) => Some((conn, root)),
            None => {
                self.notice = Some(tr!("db-no-connection"));
                None
            }
        }
    }

    /// Runs the selection, or the SQL statement at the caret of a `.sql`
    /// file; rows arrive as [`Message::SqlResult`].
    pub(crate) fn run_sql(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let sql = match doc.selection() {
            Some((a, b)) if a < b => doc.text[a..b].trim().to_string(),
            _ if doc.file_type == crate::filetype::FileType::Sql => {
                database::statement_at(&doc.text, doc.offset(doc.cursor())).to_string()
            }
            _ => {
                self.notice = Some(tr!("db-select-sql"));
                return Task::none();
            }
        };
        if sql.is_empty() {
            return Task::none();
        }
        self.query_database(sql)
    }

    /// Sends `sql` to the database.
    fn query_database(&mut self, sql: String) -> Task<Message> {
        let Some((conn, root)) = self.connection() else {
            return Task::none();
        };
        self.notice = Some(tr!("db-running", name = conn.name.as_str()));
        let driver = conn.driver;
        let shown = sql.clone();
        Task::perform(database::run(conn, root, sql), move |result| {
            Message::SqlResult(shown, driver, result)
        })
    }

    /// Opens the rows of a finished query as a table in a tab.
    pub(crate) fn sql_result(
        &mut self,
        sql: &str,
        driver: database::Driver,
        result: Result<String, String>,
    ) -> Task<Message> {
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                self.fail(tr!("db-failed", error = error));
                return Task::none();
            }
        };
        let rows = database::rows(&output, driver);
        let table = if rows.is_empty() {
            tr!("db-no-rows")
        } else {
            database::markdown_table(&rows, &tr!("db-more-rows"))
        };
        let count = rows.len().saturating_sub(1);
        let text = format!(
            "```sql\n{sql}\n```\n\n{}\n\n{table}",
            tr!("db-row-count", count = count)
        );
        let dir = std::env::temp_dir().join("phpthunder-sql");
        let path = dir.join("result.md");
        match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, text)) {
            Ok(()) => {
                self.notice = None;
                self.open(path, None)
            }
            Err(error) => {
                self.fail(error.to_string());
                Task::none()
            }
        }
    }

    /// Lists the database's tables ([`Message::TablesLoaded`]).
    pub(crate) fn database_tables(&mut self) -> Task<Message> {
        let Some((conn, root)) = self.connection() else {
            return Task::none();
        };
        let driver = conn.driver;
        let sql = database::tables_sql(driver).to_string();
        Task::perform(database::run(conn, root, sql), move |result| {
            Message::TablesLoaded(driver, result)
        })
    }

    /// Shows the table picker; picking one queries its first rows.
    pub(crate) fn tables_loaded(
        &mut self,
        driver: database::Driver,
        result: Result<String, String>,
    ) -> Task<Message> {
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                self.fail(tr!("db-failed", error = error));
                return Task::none();
            }
        };
        let items = database::rows(&output, driver)
            .into_iter()
            .skip(1)
            .filter_map(|r| r.into_iter().next())
            .map(|table| Item {
                badge: "▦",
                title: table.clone(),
                matched: Vec::new(),
                detail: String::new(),
                target: Target::QueryTable(table),
            })
            .collect();
        self.open_choice(tr!("db-tables-title"), items)
    }

    /// Shows the first rows of `table`.
    pub(crate) fn query_table(&mut self, table: String) -> Task<Message> {
        let quoted = table.replace('"', "");
        let sql = match self
            .tree
            .root
            .as_deref()
            .and_then(|r| database::connections(r).into_iter().next())
            .map(|c| c.driver)
        {
            Some(database::Driver::Mysql) => {
                format!("SELECT * FROM `{}` LIMIT 100;", quoted.replace('`', ""))
            }
            _ => format!("SELECT * FROM \"{quoted}\" LIMIT 100;"),
        };
        self.query_database(sql)
    }
}
