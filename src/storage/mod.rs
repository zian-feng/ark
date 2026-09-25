mod schema;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

pub const MAX_DESCRIPTION_LENGTH: usize = 1_000;

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
    pub cwd: PathBuf,
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

pub struct SessionChanges {
    pub alias: Option<String>,
    pub description: Option<String>,
    pub provider: Option<String>,
    pub cwd: Option<PathBuf>,
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
        if description.chars().count() > MAX_DESCRIPTION_LENGTH {
            anyhow::bail!("description must be at most {MAX_DESCRIPTION_LENGTH} characters");
        }
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

    pub fn set_starred(&self, key: &str, starred: bool) -> Result<()> {
        let updated_rows = self.connection.execute(
            r#"
            UPDATE sessions
            SET starred = ?1
            WHERE rowid = (
                SELECT rowid
                FROM sessions
                WHERE id = ?2 OR session_id = ?2
                ORDER BY CASE WHEN id = ?2 THEN 0 ELSE 1 END
                LIMIT 1
            )
            "#,
            params![if starred { 1 } else { 0 }, key],
        )?;

        if updated_rows == 0 {
            anyhow::bail!("no saved session with alias or ID `{key}`");
        }

        Ok(())
    }

    pub fn update_session(&mut self, key: &str, changes: SessionChanges) -> Result<()> {
        if changes.alias.is_none()
            && changes.description.is_none()
            && changes.provider.is_none()
            && changes.cwd.is_none()
        {
            anyhow::bail!("provide at least one field to update");
        }

        if changes.provider.is_some() != changes.cwd.is_some() {
            anyhow::bail!("provider and cwd must be updated together");
        }

        if changes
            .description
            .as_deref()
            .is_some_and(|description| description.chars().count() > MAX_DESCRIPTION_LENGTH)
        {
            anyhow::bail!("description must be at most {MAX_DESCRIPTION_LENGTH} characters");
        }

        let transaction = self.connection.transaction()?;
        let rowid = transaction
            .query_row(
                r#"
                SELECT rowid
                FROM sessions
                WHERE id = ?1 OR session_id = ?1
                ORDER BY CASE WHEN id = ?1 THEN 0 ELSE 1 END
                LIMIT 1
                "#,
                params![key],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .with_context(|| format!("no saved session with alias or ID `{key}`"))?;

        if let Some(alias) = &changes.alias {
            let alias_owner = transaction
                .query_row(
                    "SELECT rowid FROM sessions WHERE id = ?1",
                    params![alias],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;

            if alias_owner.is_some_and(|owner| owner != rowid) {
                anyhow::bail!("alias `{alias}` is already used by another saved session");
            }
        }

        let cwd = changes
            .cwd
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());

        transaction.execute(
            r#"
            UPDATE sessions
            SET
                id = COALESCE(?1, id),
                description = COALESCE(?2, description),
                provider = COALESCE(?3, provider),
                cwd = COALESCE(?4, cwd)
            WHERE rowid = ?5
            "#,
            params![
                changes.alias.as_deref(),
                changes.description.as_deref(),
                changes.provider.as_deref(),
                cwd.as_deref(),
                rowid,
            ],
        )?;

        transaction.commit()?;
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
                SELECT session_id, provider, cwd
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
                        cwd: PathBuf::from(row.get::<_, String>(2)?),
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

    use super::{Database, MAX_DESCRIPTION_LENGTH, NewSession, SessionChanges};

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
    fn rejects_a_description_that_exceeds_the_storage_limit() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let database = Database::open_at(temporary_directory.path().join("ark.db"))?;

        let error = database
            .add_session(NewSession {
                id: "auth-refactor".to_owned(),
                session_id: "raw-session-123".to_owned(),
                provider: "codex".to_owned(),
                cwd: PathBuf::from("/tmp/example-project"),
                description: Some("x".repeat(MAX_DESCRIPTION_LENGTH + 1)),
                tags: None,
                starred: false,
            })
            .expect_err("an overlong description should be rejected");

        assert!(error.to_string().contains("at most"));

        Ok(())
    }

    #[test]
    fn updates_alias_description_provider_and_cwd_together() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let mut database = Database::open_at(temporary_directory.path().join("ark.db"))?;

        database.add_session(NewSession {
            id: "raw-session-123".to_owned(),
            session_id: "raw-session-123".to_owned(),
            provider: "codex".to_owned(),
            cwd: PathBuf::from("/tmp/codex-project"),
            description: Some("Old description".to_owned()),
            tags: None,
            starred: false,
        })?;

        database.update_session(
            "raw-session-123",
            SessionChanges {
                alias: Some("auth-refactor".to_owned()),
                description: Some("New description".to_owned()),
                provider: Some("claude".to_owned()),
                cwd: Some(PathBuf::from("/tmp/claude-project")),
            },
        )?;

        let session = database.get_resume_session("auth-refactor")?;
        let summary = database
            .list_sessions()?
            .pop()
            .expect("session should exist");

        assert_eq!(session.session_id, "raw-session-123");
        assert_eq!(session.provider, "claude");
        assert_eq!(session.cwd, PathBuf::from("/tmp/claude-project"));
        assert_eq!(summary.id, "auth-refactor");
        assert_eq!(summary.description, "New description");

        Ok(())
    }

    #[test]
    fn rejects_an_alias_owned_by_another_session_without_changes() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let mut database = Database::open_at(temporary_directory.path().join("ark.db"))?;

        for (id, session_id) in [
            ("first-session", "raw-session-1"),
            ("second-session", "raw-session-2"),
        ] {
            database.add_session(NewSession {
                id: id.to_owned(),
                session_id: session_id.to_owned(),
                provider: "codex".to_owned(),
                cwd: PathBuf::from("/tmp/example-project"),
                description: None,
                tags: None,
                starred: false,
            })?;
        }

        let error = database
            .update_session(
                "second-session",
                SessionChanges {
                    alias: Some("first-session".to_owned()),
                    description: Some("Should not be saved".to_owned()),
                    provider: None,
                    cwd: None,
                },
            )
            .expect_err("an existing alias should be rejected");

        assert!(error.to_string().contains("already used"));
        assert_eq!(
            database.list_sessions()?[0].description,
            "",
            "the failed update must not change the target row"
        );

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
        assert_eq!(by_alias.cwd, PathBuf::from("/tmp/example-project"));

        Ok(())
    }

    #[test]
    fn sets_starred_status_by_alias_or_native_id() -> anyhow::Result<()> {
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

        database.set_starred("auth-refactor", true)?;
        assert!(database.list_sessions()?[0].starred);

        database.set_starred("raw-session-123", false)?;
        assert!(!database.list_sessions()?[0].starred);

        Ok(())
    }
}
