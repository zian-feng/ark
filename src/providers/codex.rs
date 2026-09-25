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

        let metadata = read_session_metadata(&file_path)?;
        if metadata
            .session_id
            .as_deref()
            .is_some_and(|id| id != session_id)
        {
            return Ok(None);
        }
        let Some(cwd) = metadata.cwd else {
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
            preview: metadata.preview,
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

struct SessionMetadata {
    session_id: Option<String>,
    cwd: Option<PathBuf>,
    preview: Option<String>,
}

fn read_session_metadata(path: &Path) -> Result<SessionMetadata> {
    let file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut session_id = None;
    let mut cwd = None;
    let mut first_user_message = None;
    let mut preview = None;

    for line in BufReader::new(file).lines() {
        let value: Value = serde_json::from_str(&line?)?;

        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            let payload = value.get("payload").unwrap_or(&Value::Null);
            if session_id.is_none() {
                session_id = payload
                    .get("id")
                    .or_else(|| payload.get("session_id"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
            }
            if cwd.is_none() {
                cwd = payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(PathBuf::from);
            }
        }

        if let Some(message) = user_message(&value) {
            if first_user_message.is_none() {
                first_user_message = Some(message.clone());
            }

            if preview.is_none() && !is_session_control_command(&message) {
                preview = Some(message);
            }
        }
    }

    Ok(SessionMetadata {
        session_id,
        cwd,
        preview: preview.or(first_user_message),
    })
}

fn user_message(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) != Some("event_msg") {
        return None;
    }

    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("user_message") {
        return None;
    }

    if payload
        .get("kind")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind != "plain")
    {
        return None;
    }

    payload
        .get("message")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn is_session_control_command(message: &str) -> bool {
    matches!(
        message.trim().split_whitespace().next(),
        Some("/permissions" | "/model" | "/reasoning" | "/compact" | "/recap")
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::read_session_metadata;

    #[test]
    fn skips_session_control_commands_when_choosing_a_preview() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let path = temporary_directory.path().join("session.jsonl");
        fs::write(
            &path,
            concat!(
                r#"{"type":"session_meta","payload":{"session_id":"session-1","cwd":"/work/example"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","kind":"plain","message":"/permissions read-only"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","kind":"plain","message":"Add rate limiting"}}"#,
                "\n"
            ),
        )
        .unwrap();

        let metadata = read_session_metadata(&path).unwrap();

        assert_eq!(metadata.session_id.as_deref(), Some("session-1"));
        assert_eq!(metadata.cwd.as_deref(), Some(Path::new("/work/example")));
        assert_eq!(metadata.preview.as_deref(), Some("Add rate limiting"));
    }

    #[test]
    fn uses_the_first_prompt_when_every_prompt_is_a_control_command() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let path = temporary_directory.path().join("session.jsonl");
        fs::write(
            &path,
            concat!(
                r#"{"type":"event_msg","payload":{"type":"user_message","kind":"plain","message":"/permissions read-only"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","kind":"plain","message":"/model gpt-5"}}"#,
                "\n"
            ),
        )
        .unwrap();

        let metadata = read_session_metadata(&path).unwrap();

        assert_eq!(metadata.preview.as_deref(), Some("/permissions read-only"));
    }
}
