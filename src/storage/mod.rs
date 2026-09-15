mod schema;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub session_id: String,
    pub provider: String,
    pub cwd: PathBuf,
    pub description: String,
    pub tags: Option<Vec<String>>,
    pub starred: bool,
    pub created_at: DateTime<Utc>,
    pub last_opened_at: Option<DateTime<Utc>>,
}

pub struct NewSession {
    pub id: String,
    pub session_id: String,
    pub provider: String,
    pub cwd: PathBuf,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub starred: bool,
}

pub struct Database {
    connection: Connection,
}

impl Database {
    /// opens persistent database at ~/.ark/ark.db
    pub fn open() -> Result<Self> {
        let home = dirs::home_dir().context("could not determine the home directory")?;
        let database_path = home.join(".ark").join("ark.db");

        Self::open_at(database_path)
    }

    /// open a db from an explicit path for testing

    pub fn open_at(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create Ark data directory:
                  {}", parent.display()))?;
        }

        let connection = Connection::open(path)
            .with_context(|| format!("could not open Ark database: {}",
              path.display()))?;
        
        schema::initialize(&connection)?;

        Ok(Self { connection })
    }

    pub fn path_for_current_user() -> Result<PathBuf> {
        let home = dirs::home_dir().context("could not determine the home
          directory")?;
        Ok(home.join(".ark").join("ark.db"))
    }

    pub fn add_session(&self, new_session: NewSession) -> Result<Session> {
        let created_at = Utc::now();
        let created_at_text = created_at.to_rfc3339();
        let description = new_session.description.unwrap_or_default();
        let tags = new_session.tags.unwrap_or_default();
        let tags_json = serde_json::to_string(&tags)?;
        let cwd = new_session.cwd.to_string_lossy().into_owned();
        let starred = if new_session.starred { 1 } else { 0 };

        self.connection
            .execute(
                r#"
                INSERT INTO sessions (
                    id, session_id, provider, cwd, description, tags,
                    starred, created_at, last_opened_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
                "#,
                params![
                    &new_session.id,
                    &new_session.session_id,
                    &new_session.provider,
                    &cwd,
                    &description,
                    &tags_json,
                    starred,
                    &created_at_text,
                ],
            )
            .context("could not save session")?;

        Ok(Session {
            id: new_session.id,
            session_id: new_session.session_id,
            provider: new_session.provider,
            cwd: new_session.cwd,
            description,
            tags: Some(tags),
            starred: new_session.starred,
            created_at,
            last_opened_at: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Database, NewSession};

    #[test]
    fn opening_db_creates_session_table() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let database_path = temporary_directory.path().join("ark.db");

        let database = Database::open_at(&database_path)?;

        let table_name: String = database.connection.query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'sessions'",
            [],
            |row| row.get(0),
        )?;

        assert_eq!(table_name, "sessions");

        Ok(())
    }

    #[test]
    fn adds_a_session() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let database = Database::open_at(temporary_directory.path().join("ark.db"))?;

        let saved = database.add_session(NewSession {
            id: "raw-session-123".to_owned(),
            session_id: "raw-session-123".to_owned(),
            provider: "codex".to_owned(),
            cwd: PathBuf::from("/tmp/example-project"),
            description: None,
            tags: None,
            starred: false,
        })?;

        assert_eq!(saved.id, "raw-session-123");
        assert_eq!(saved.provider, "codex");
        assert_eq!(saved.description, "");
        assert_eq!(saved.tags, Some(vec![]));

        Ok(())
    }
}
