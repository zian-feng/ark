use crate::storage::Database;
use anyhow::{Context, Result, bail};
use std::process::Command;

pub fn open_session(key: &str) -> Result<()> {
    let database = Database::open()?;
    let target = database.get_resume_session(key)?;

    if target.provider != "codex" {
        bail!("cannot resume unknown provider `{}`", target.provider);
    }

    let exit_status = Command::new("codex")
        .arg("resume")
        .arg(&target.session_id)
        .status()
        .context("could not launch Codex")?;

    if !exit_status.success() {
        bail!("Codex exited with status {exit_status}");
    }

    Ok(())
}
