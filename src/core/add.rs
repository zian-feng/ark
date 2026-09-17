use crate::{
    slug::slugify,
    storage::{Database, NewSession, Session},
};

use anyhow::{Context, Result, bail};
use std::env;

pub fn add_new_session_codex(session_id: &str, alias: Option<&str>) -> Result<Session> {
    let cwd = env::current_dir().context("could not determine the current directory")?;
    let database = Database::open()?;

    let id = match alias {
        Some(alias) => {
            let id = slugify(alias);

            if id.is_empty() {
                bail!("alias must contain at least one letter or number")
            }

            id
        }

        None => session_id.to_owned(),
    };

    database.add_session(NewSession {
        id,
        session_id: session_id.to_owned(),
        provider: "codex".to_owned(),
        cwd,
        description: None,
        tags: None,
        starred: false,
    })
}
