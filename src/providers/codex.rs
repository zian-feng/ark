use std::{
    env,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::{
    DiscoveredSession, HandoffContext, Provider, SessionIdHint, binary_on_path, is_non_task_prompt,
};

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
        let root = codex_sessions_dir()?;
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
        }))
    }

    fn find_recent_session(
        &self,
        cwd: &Path,
        started_at: DateTime<Utc>,
    ) -> Result<Option<DiscoveredSession>> {
        find_recent_rollout(&codex_sessions_dir()?, cwd, started_at, self.name())
    }

    fn extract_handoff(&self, session_id: &str) -> Result<Option<HandoffContext>> {
        let Some(path) = find_rollout_file(&codex_sessions_dir()?, session_id)? else {
            return Ok(None);
        };
        read_handoff_context(&path, session_id)
    }

    fn resume_command(&self, session_id: &str) -> Command {
        let mut command = Command::new("codex");
        command.arg("resume").arg(session_id);
        command
    }

    fn new_session_command(&self, initial_prompt: &str) -> Command {
        let mut command = Command::new("codex");
        command.arg(initial_prompt);
        command
    }
}

fn codex_sessions_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(path).join("sessions"));
    }
    let home = dirs::home_dir().context("could not determine the home directory")?;
    Ok(home.join(".codex").join("sessions"))
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

fn find_recent_rollout(
    root: &Path,
    cwd: &Path,
    started_at: DateTime<Utc>,
    provider: &'static str,
) -> Result<Option<DiscoveredSession>> {
    if !root.exists() {
        return Ok(None);
    }

    let mut newest = None;
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("could not read {}", directory.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                directories.push(path);
                continue;
            }
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
            {
                continue;
            }
            let modified = fs::metadata(&path)?
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let modified_at = DateTime::<Utc>::from(modified);
            if modified_at <= started_at {
                continue;
            }
            let metadata = read_session_metadata(&path)?;
            let (Some(session_id), Some(session_cwd)) = (metadata.session_id, metadata.cwd) else {
                continue;
            };
            if session_cwd != cwd {
                continue;
            }
            let session = DiscoveredSession {
                session_id,
                provider,
                cwd: session_cwd,
                modified_at,
                preview: metadata.preview,
            };
            if newest.as_ref().is_none_or(|existing: &DiscoveredSession| {
                session.modified_at > existing.modified_at
            }) {
                newest = Some(session);
            }
        }
    }
    Ok(newest)
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

            if preview.is_none() && !is_non_task_prompt(&message) {
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

fn read_handoff_context(path: &Path, expected_session_id: &str) -> Result<Option<HandoffContext>> {
    let metadata = read_session_metadata(path)?;
    if metadata
        .session_id
        .as_deref()
        .is_some_and(|session_id| session_id != expected_session_id)
        || metadata.cwd.is_none()
    {
        return Ok(None);
    }

    let file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut requests = Vec::new();
    for line in BufReader::new(file).lines() {
        let value: Value = serde_json::from_str(&line?)?;
        if let Some(message) = user_message(&value).filter(|message| !is_non_task_prompt(message)) {
            if requests.last() != Some(&message) {
                requests.push(message);
            }
        }
    }

    let recent_user_requests = requests.into_iter().rev().take(4).collect::<Vec<_>>();
    Ok(Some(HandoffContext {
        title: metadata.preview,
        summary: None,
        recent_user_requests: recent_user_requests.into_iter().rev().collect(),
    }))
}

fn user_message(value: &Value) -> Option<String> {
    let payload = value.get("payload")?;

    match value.get("type").and_then(Value::as_str) {
        Some("event_msg")
            if payload.get("type").and_then(Value::as_str) == Some("user_message") =>
        {
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
        Some("response_item")
            if payload.get("type").and_then(Value::as_str) == Some("message")
                && payload.get("role").and_then(Value::as_str) == Some("user") =>
        {
            payload
                .get("content")
                .and_then(Value::as_array)?
                .iter()
                .find(|item| item.get("type").and_then(Value::as_str) == Some("input_text"))?
                .get("text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use chrono::{Duration, Utc};

    use super::{find_recent_rollout, read_handoff_context, read_session_metadata};

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

    #[test]
    fn reads_user_prompts_from_response_items() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let path = temporary_directory.path().join("session.jsonl");
        fs::write(
            &path,
            concat!(
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"/permissions read-only"}]}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context> <cwd>/work/example</cwd> </environment_context>"}]}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"."}]}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Implement automatic descriptions"}]}}"#,
                "\n"
            ),
        )
        .unwrap();

        let metadata = read_session_metadata(&path).unwrap();

        assert_eq!(
            metadata.preview.as_deref(),
            Some("Implement automatic descriptions")
        );
    }

    #[test]
    fn finds_the_newest_recent_session_for_a_directory() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let sessions = temporary_directory.path().join("sessions/2026/10/20");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(
            sessions.join("rollout-test-session-1.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"session-1","cwd":"/work/example"}}"#,
        )
        .unwrap();

        let found = find_recent_rollout(
            &temporary_directory.path().join("sessions"),
            Path::new("/work/example"),
            Utc::now() - Duration::seconds(1),
            "codex",
        )
        .unwrap()
        .expect("recent session should be found");

        assert_eq!(found.session_id, "session-1");
        assert_eq!(found.cwd, Path::new("/work/example"));
    }

    #[test]
    fn extracts_recent_meaningful_requests_for_a_handoff() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let path = temporary_directory.path().join("session.jsonl");
        fs::write(
            &path,
            concat!(
                r#"{"type":"session_meta","payload":{"id":"session-1","cwd":"/work/example"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","kind":"plain","message":"/permissions read-only"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","kind":"plain","message":"Fix login flow"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","kind":"plain","message":"Add regression tests"}}"#,
                "\n"
            ),
        )
        .unwrap();

        let handoff = read_handoff_context(&path, "session-1")
            .unwrap()
            .expect("handoff should be available");

        assert_eq!(handoff.title.as_deref(), Some("Fix login flow"));
        assert_eq!(
            handoff.recent_user_requests,
            ["Fix login flow", "Add regression tests"]
        );
    }
}
