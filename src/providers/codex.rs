use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::{DiscoveredSession, Provider, SessionIdHint, binary_on_path};

pub struct CodexProvider;

impl Provider for CodexProvider {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn is_available(&self) -> bool {
        binary_on_path("codex")
    }

    fn id_hint_priority(&self, hint: SessionIdHint) -> u8 {
        match hint {
            SessionIdHint::UuidV7 => 0,
            _ => 10,
        }
    }

    fn find_session_by_id(&self, session_id: &str) -> Result<Option<DiscoveredSession>> {
        let Some(home) = dirs::home_dir() else {
            return Ok(None);
        };
        let root = home.join(".codex").join("sessions");
        let Some(file_path) = find_rollout_file(&root, session_id)? else {
            return Ok(None);
        };

        let (metadata_id, cwd) = read_session_metadata(&file_path)?;
        if metadata_id.as_deref().is_some_and(|id| id != session_id) {
            return Ok(None);
        }
        let Some(cwd) = cwd else {
            return Ok(None);
        };

        let modified = fs::metadata(&file_path)
            .with_context(|| format!("could not read {}", file_path.display()))?
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);

        Ok(Some(DiscoveredSession {
            session_id: session_id.to_owned(),
            provider: self.name(),
            cwd,
            modified_at: DateTime::<Utc>::from(modified),
            preview: None,
            file_path,
        }))
    }

    fn resume_command(&self, session_id: &str) -> Command {
        let mut command = Command::new("codex");
        command.arg("resume").arg(session_id);
        command
    }
}

fn find_rollout_file(root: &Path, session_id: &str) -> Result<Option<PathBuf>> {
    if !root.exists() {
        return Ok(None);
    }

    let mut directories = vec![root.to_path_buf()];
    let suffix = format!("-{session_id}.jsonl");

    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("could not read {}", directory.display()))?
        {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                directories.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(&suffix))
            {
                return Ok(Some(path));
            }
        }
    }

    Ok(None)
}

fn read_session_metadata(path: &Path) -> Result<(Option<String>, Option<PathBuf>)> {
    let file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;

    for line in BufReader::new(file).lines() {
        let value: Value = serde_json::from_str(&line?)?;
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            continue;
        }

        let payload = value.get("payload").unwrap_or(&Value::Null);
        let session_id = payload
            .get("id")
            .or_else(|| payload.get("session_id"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let cwd = payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(PathBuf::from);

        return Ok((session_id, cwd));
    }

    Ok((None, None))
}
