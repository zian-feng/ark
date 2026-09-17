mod schema;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub starred: bool,
    pub id: String,
    pub session_id: String,
    pub provider: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeSession {
    pub session_id: String,
    pub provider: String,
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
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "could not create Ark data directory:
                  {}",
                    parent.display()
                )
            })?;
        }

        let connection = Connection::open(path)
            .with_context(|| format!("could not open Ark database: {}", path.display()))?;

        schema::initialize(&connection)?;

        Ok(Self { connection })
    }

    pub fn path_for_current_user() -> Result<PathBuf> {
        let home = dirs::home_dir().context(
            "could not determine the home
          directory",
        )?;
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

    pub fn remove_session(&self, key: &str) -> Result<()> {
        let deleted_rows = self.connection.execute(
            r#"
                DELETE FROM sessions
                WHERE rowid = (
                    SELECT rowid
                    FROM sessions
                    WHERE id = ?1 OR session_id = ?1
                    ORDER BY CASE WHEN id = ?1 THEN 0 ELSE 1 END
                    LIMIT 1
                )
                "#,
            params![key],
        )?;

        if deleted_rows == 0 {
            anyhow::bail!("no saved session with alias or ID `{key}`");
        }

        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT starred, id, session_id, provider, description
            FROM sessions
            ORDER BY
                starred DESC,
                COALESCE(last_opened_at, created_at) DESC
            "#,
        )?;

        let rows = statement.query_map([], |row| {
            Ok(SessionSummary {
                starred: row.get::<_, i64>(0)? != 0,
                id: row.get(1)?,
                session_id: row.get(2)?,
                provider: row.get(3)?,
                description: row.get(4)?,
            })
        })?;

        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_resume_session(&self, key: &str) -> Result<ResumeSession> {
        let target = self
            .connection
            .query_row(
                r#"
                SELECT session_id, provider
                FROM sessions
                WHERE id = ?1 OR session_id = ?1
                ORDER BY CASE WHEN id = ?1 THEN 0 ELSE 1 END
                LIMIT 1
                "#,
                params![key],
                |row| {
                    Ok(ResumeSession {
                        session_id: row.get(0)?,
                        provider: row.get(1)?,
                    })
                },
            )
            .optional()?
            .with_context(|| format!("no saved session with alias or ID `{key}`"))?;

        Ok(target)
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
            id: "auth-refactor".to_owned(),
            session_id: "raw-session-123".to_owned(),
            provider: "codex".to_owned(),
            cwd: PathBuf::from("/tmp/example-project"),
            description: None,
            tags: None,
            starred: false,
        })?;

        assert_eq!(saved.id, "auth-refactor");
        assert_eq!(saved.provider, "codex");
        assert_eq!(saved.description, "");
        assert_eq!(saved.tags, Some(vec![]));

        Ok(())
    }

    #[test]
    fn removes_a_session() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let database = Database::open_at(temporary_directory.path().join("ark.db"))?;

        database.add_session(NewSession {
            id: "auth-refactor".to_owned(),
            session_id: "raw-session-123".to_owned(),
            provider: "codex".to_owned(),
            cwd: PathBuf::from("/tmp/example-project"),
            description: None,
            tags: None,
            starred: false,
        })?;

        database.remove_session("auth-refactor")?;

        assert!(database.list_sessions()?.is_empty());

        Ok(())
    }

    #[test]
    fn removes_a_session_by_native_id() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let database = Database::open_at(temporary_directory.path().join("ark.db"))?;

        database.add_session(NewSession {
            id: "auth-refactor".to_owned(),
            session_id: "raw-session-123".to_owned(),
            provider: "codex".to_owned(),
            cwd: PathBuf::from("/tmp/example-project"),
            description: None,
            tags: None,
            starred: false,
        })?;

        database.remove_session("raw-session-123")?;

        assert!(database.list_sessions()?.is_empty());

        Ok(())
    }

    #[test]
    fn lists_session_summaries() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let database = Database::open_at(temporary_directory.path().join("ark.db"))?;

        database.add_session(NewSession {
            id: "raw-session-123".to_owned(),
            session_id: "raw-session-123".to_owned(),
            provider: "codex".to_owned(),
            cwd: PathBuf::from("/tmp/example-project"),
            description: Some("Example session".to_owned()),
            tags: None,
            starred: false,
        })?;

        let sessions = database.list_sessions()?;

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "raw-session-123");
        assert_eq!(sessions[0].session_id, "raw-session-123");
        assert_eq!(sessions[0].provider, "codex");
        assert_eq!(sessions[0].description, "Example session");
        assert!(!sessions[0].starred);

        Ok(())
    }

    #[test]
    fn finds_a_resume_session_by_alias_or_native_id() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let database = Database::open_at(temporary_directory.path().join("ark.db"))?;

        database.add_session(NewSession {
            id: "auth-refactor".to_owned(),
            session_id: "raw-session-123".to_owned(),
            provider: "codex".to_owned(),
            cwd: PathBuf::from("/tmp/example-project"),
            description: None,
            tags: None,
            starred: false,
        })?;

        let by_alias = database.get_resume_session("auth-refactor")?;
        let by_native_id = database.get_resume_session("raw-session-123")?;

        assert_eq!(by_alias.session_id, "raw-session-123");
        assert_eq!(by_native_id.session_id, "raw-session-123");
        assert_eq!(by_alias.provider, "codex");

        Ok(())
    }
}
