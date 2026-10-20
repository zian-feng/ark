use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PendingSession {
    pub provider: String,
    pub cwd: PathBuf,
    pub started_at: DateTime<Utc>,
}

pub fn write(pid: u32, provider: &str, cwd: PathBuf) -> Result<()> {
    let path = path_for_pid(pid)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "could not create Ark pending directory: {}",
                parent.display()
            )
        })?;
    }
    let pending = PendingSession {
        provider: provider.to_owned(),
        cwd,
        started_at: Utc::now(),
    };
    fs::write(&path, serde_json::to_string(&pending)?)
        .with_context(|| format!("could not write Ark pending session: {}", path.display()))
}

/// Reads and clears this shell's pending session, even if parsing fails.
pub fn take(pid: u32) -> Result<Option<PendingSession>> {
    let path = path_for_pid(pid)?;
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("could not read Ark pending session: {}", path.display())
            });
        }
    };
    fs::remove_file(&path)
        .with_context(|| format!("could not clear Ark pending session: {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("could not parse Ark pending session: {}", path.display()))
        .map(Some)
}

fn path_for_pid(pid: u32) -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine the home directory")?;
    Ok(home
        .join(".ark")
        .join("pending")
        .join(format!("{pid}.json")))
}

#[cfg(test)]
mod tests {
    // Pending records are intentionally keyed by shell PID. Filesystem tests for
    // the real home directory would be unsafe, so config-level tests cover the
    // serialisation dependency and this module stays deliberately small.
}
