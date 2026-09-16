use anyhow::Result;
use rusqlite::Connection;

const CREATE_SESSION_TABLE: &str = r#"
    CREATE TABLE IF NOT EXISTS sessions(
        id TEXT PRIMARY KEY NOT NULL,
        session_id TEXT NOT NULL,
        provider TEXT NOT NULL,
        cwd TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        tags TEXT NOT NULL DEFAULT '[]',
        starred INTEGER NOT NULL DEFAULT 0 CHECK (starred IN (0, 1)),
        created_at TEXT NOT NULL,
        last_opened_at TEXT,

        UNIQUE(provider, session_id)
    );
"#;

const CREATE_PROVIDER_INDEX: &str = r#"
    CREATE INDEX IF NOT EXISTS idx_session_provider
    ON sessions(provider);
"#;

const CREATE_STARRED_INDEX: &str = r#"
    CREATE INDEX IF NOT EXISTS idx_session_starred
    ON sessions(starred DESC, last_opened_at DESC);
"#;

pub fn initialize(connection: &Connection) -> Result<()> {
    connection.execute_batch(CREATE_SESSION_TABLE)?;
    connection.execute_batch(CREATE_PROVIDER_INDEX)?;
    connection.execute_batch(CREATE_STARRED_INDEX)?;

    Ok(())
}
