use crate::storage::{Database, NewSession, Session};
use anyhow::{Context, Result};
use std::env;

pub fn add_new_session_codex(session_id: &str) -> Result<Session> {
    let cwd = env::current_dir().context("could not determine the current directory")?;
    let database = Database::open()?;

    database.add_session(NewSession {
        id: session_id.to_owned(),
        session_id: session_id.to_owned(),
        provider: "codex".to_owned(),
        cwd,
        description: None,
        tags: None,
        starred: false,
    })
}
