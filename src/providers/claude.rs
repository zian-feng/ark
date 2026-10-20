use std::{
    env, fs,
    fs::File,
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

pub struct ClaudeProvider;

impl Provider for ClaudeProvider {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn is_available(&self) -> bool {
        binary_on_path("claude")
    }

    fn id_hint_priority(&self, hint: SessionIdHint) -> u8 {
        match hint {
            SessionIdHint::UuidV4 => 0,
            _ => 10,
        }
    }

    fn find_session_by_id(&self, session_id: &str) -> Result<Option<DiscoveredSession>> {
        let root = claude_data_dir()?.join("projects");
        find_session_in_projects(&root, session_id, self.name())
    }

    fn find_recent_session(
        &self,
        cwd: &Path,
        started_at: DateTime<Utc>,
    ) -> Result<Option<DiscoveredSession>> {
        find_recent_session_in_projects(
            &claude_data_dir()?.join("projects"),
            cwd,
            started_at,
            self.name(),
        )
    }

    fn extract_handoff(&self, session_id: &str) -> Result<Option<HandoffContext>> {
        let root = claude_data_dir()?.join("projects");
        let Some(path) = find_session_file(&root, session_id)? else {
            return Ok(None);
        };
        read_handoff_context(&path, session_id)
    }

    fn resume_command(&self, session_id: &str) -> Command {
        let mut command = Command::new("claude");
        command.arg("--resume").arg(session_id);
        command
    }

    fn new_session_command(&self, initial_prompt: &str) -> Command {
        let mut command = Command::new("claude");
        command.arg(initial_prompt);
        command
    }
}

fn claude_data_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }

    let home = dirs::home_dir().context("could not determine the home directory")?;
    Ok(home.join(".claude"))
}

fn find_session_in_projects(
    projects_root: &Path,
    session_id: &str,
    provider: &'static str,
) -> Result<Option<DiscoveredSession>> {
    if !projects_root.exists() {
        return Ok(None);
    }

    let target_name = format!("{session_id}.jsonl");
    let mut directories = vec![projects_root.to_path_buf()];

    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("could not read {}", directory.display()))?
        {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                directories.push(path);
                continue;
            }

            if path.file_name().and_then(|name| name.to_str()) != Some(target_name.as_str()) {
                continue;
            }

            let metadata = read_session_metadata(&path, session_id)?;
            let Some(metadata) = metadata else {
                continue;
            };
            let Some(cwd) = metadata.cwd else {
                continue;
            };

            let modified = fs::metadata(&path)
                .with_context(|| format!("could not read {}", path.display()))?
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH);

            return Ok(Some(DiscoveredSession {
                session_id: session_id.to_owned(),
                provider,
                cwd,
                modified_at: DateTime::<Utc>::from(modified),
                preview: metadata.title,
            }));
        }
    }

    Ok(None)
}

fn find_session_file(projects_root: &Path, session_id: &str) -> Result<Option<PathBuf>> {
    if !projects_root.exists() {
        return Ok(None);
    }

    let target_name = format!("{session_id}.jsonl");
    let mut directories = vec![projects_root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("could not read {}", directory.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                directories.push(path);
            } else if path.file_name().and_then(|name| name.to_str()) == Some(&target_name) {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

fn find_recent_session_in_projects(
    projects_root: &Path,
    cwd: &Path,
    started_at: DateTime<Utc>,
    provider: &'static str,
) -> Result<Option<DiscoveredSession>> {
    if !projects_root.exists() {
        return Ok(None);
    }

    let mut newest = None;
    let mut directories = vec![projects_root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("could not read {}", directory.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                directories.push(path);
                continue;
            }
            let Some(session_id) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                continue;
            }
            let modified = fs::metadata(&path)?
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let modified_at = DateTime::<Utc>::from(modified);
            if modified_at <= started_at {
                continue;
            }
            let Some(metadata) = read_session_metadata(&path, session_id)? else {
                continue;
            };
            let Some(session_cwd) = metadata.cwd else {
                continue;
            };
            if session_cwd != cwd {
                continue;
            }
            let session = DiscoveredSession {
                session_id: session_id.to_owned(),
                provider,
                cwd: session_cwd,
                modified_at,
                preview: metadata.title,
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
    cwd: Option<PathBuf>,
    title: Option<String>,
}

fn read_session_metadata(path: &Path, session_id: &str) -> Result<Option<SessionMetadata>> {
    let file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut found_session_id = false;
    let mut cwd = None;
    let mut title = None;
    let mut first_user_prompt = None;
    let mut prompt_preview = None;

    for line in BufReader::new(file).lines() {
        let value: Value = serde_json::from_str(&line?)
            .with_context(|| format!("could not parse {}", path.display()))?;

        let belongs_to_session = record_session_id(&value) == Some(session_id);
        if belongs_to_session {
            found_session_id = true;
            if cwd.is_none() {
                cwd = record_cwd(&value);
            }
        }

        if value.get("type").and_then(Value::as_str) == Some("ai-title") && belongs_to_session {
            if title.is_none() {
                title = value
                    .get("aiTitle")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
            }
        }

        if belongs_to_session {
            if let Some(prompt) = user_prompt(&value) {
                if first_user_prompt.is_none() {
                    first_user_prompt = Some(prompt.clone());
                }
                if prompt_preview.is_none() && !is_non_task_prompt(&prompt) {
                    prompt_preview = Some(prompt);
                }
            }
        }
    }

    Ok(found_session_id.then_some(SessionMetadata {
        cwd,
        title: title.or(prompt_preview.or(first_user_prompt)),
    }))
}

fn read_handoff_context(path: &Path, session_id: &str) -> Result<Option<HandoffContext>> {
    let Some(metadata) = read_session_metadata(path, session_id)? else {
        return Ok(None);
    };
    if metadata.cwd.is_none() {
        return Ok(None);
    }

    let file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut summary = None;
    let mut requests = Vec::new();
    for line in BufReader::new(file).lines() {
        let value: Value = serde_json::from_str(&line?)
            .with_context(|| format!("could not parse {}", path.display()))?;
        let belongs_to_session = record_session_id(&value) == Some(session_id);
        let is_file_local_summary = value.get("type").and_then(Value::as_str) == Some("summary")
            && record_session_id(&value).is_none();
        if value.get("type").and_then(Value::as_str) == Some("summary")
            && (belongs_to_session || is_file_local_summary)
        {
            if let Some(value) = value.get("summary").and_then(Value::as_str) {
                summary = Some(value.to_owned());
            }
        }
        if belongs_to_session
            && let Some(prompt) = user_prompt(&value).filter(|prompt| !is_non_task_prompt(prompt))
        {
            if requests.last() != Some(&prompt) {
                requests.push(prompt);
            }
        }
    }

    let recent_user_requests = requests.into_iter().rev().take(4).collect::<Vec<_>>();
    Ok(Some(HandoffContext {
        title: metadata.title,
        summary,
        recent_user_requests: recent_user_requests.into_iter().rev().collect(),
    }))
}

fn record_session_id(value: &Value) -> Option<&str> {
    value
        .get("sessionId")
        .or_else(|| value.pointer("/payload/sessionId"))
        .and_then(Value::as_str)
}

fn record_cwd(value: &Value) -> Option<PathBuf> {
    value
        .get("cwd")
        .or_else(|| value.pointer("/payload/cwd"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
}

fn user_prompt(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }

    let content = value.pointer("/message/content")?;
    match content {
        Value::String(text) => Some(text.to_owned()),
        Value::Array(items) => items
            .iter()
            .find(|item| item.get("type").and_then(Value::as_str) == Some("text"))?
            .get("text")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::{Duration, Utc};

    use super::{find_recent_session_in_projects, find_session_in_projects, read_handoff_context};

    #[test]
    fn finds_and_validates_a_claude_session_file() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let project = temporary_directory.path().join("projects/example");
        fs::create_dir_all(&project).unwrap();

        let session_id = "ffdf9b8d-61fe-4137-bdd0-2755c57796af";
        fs::write(
            project.join(format!("{session_id}.jsonl")),
            format!(
                "{{\"type\":\"user\",\"sessionId\":\"{session_id}\",\"cwd\":\"/work/example\"}}\n{{\"type\":\"ai-title\",\"sessionId\":\"{session_id}\",\"aiTitle\":\"Fix login flow\"}}\n"
            ),
        )
        .unwrap();

        let session = find_session_in_projects(
            &temporary_directory.path().join("projects"),
            session_id,
            "claude",
        )
        .unwrap()
        .expect("session should be found");

        assert_eq!(session.provider, "claude");
        assert_eq!(session.cwd, std::path::Path::new("/work/example"));
        assert_eq!(session.preview.as_deref(), Some("Fix login flow"));
    }

    #[test]
    fn uses_the_first_meaningful_user_prompt_without_an_ai_title() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let project = temporary_directory.path().join("projects/example");
        fs::create_dir_all(&project).unwrap();

        let session_id = "ffdf9b8d-61fe-4137-bdd0-2755c57796af";
        fs::write(
            project.join(format!("{session_id}.jsonl")),
            format!(
                "{{\"type\":\"user\",\"sessionId\":\"{session_id}\",\"cwd\":\"/work/example\",\"message\":{{\"content\":\"<system-reminder>metadata</system-reminder>\"}}}}\n{{\"type\":\"user\",\"sessionId\":\"{session_id}\",\"message\":{{\"content\":\"Implement Claude descriptions\"}}}}\n"
            ),
        )
        .unwrap();

        let session = find_session_in_projects(
            &temporary_directory.path().join("projects"),
            session_id,
            "claude",
        )
        .unwrap()
        .expect("session should be found");

        assert_eq!(
            session.preview.as_deref(),
            Some("Implement Claude descriptions")
        );
    }

    #[test]
    fn finds_a_recent_session_for_a_directory() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let project = temporary_directory.path().join("projects/example");
        fs::create_dir_all(&project).unwrap();
        let session_id = "ffdf9b8d-61fe-4137-bdd0-2755c57796af";
        fs::write(
            project.join(format!("{session_id}.jsonl")),
            format!(
                "{{\"type\":\"user\",\"sessionId\":\"{session_id}\",\"cwd\":\"/work/example\"}}\n"
            ),
        )
        .unwrap();

        let found = find_recent_session_in_projects(
            &temporary_directory.path().join("projects"),
            std::path::Path::new("/work/example"),
            Utc::now() - Duration::seconds(1),
            "claude",
        )
        .unwrap()
        .expect("recent session should be found");

        assert_eq!(found.session_id, session_id);
        assert_eq!(found.provider, "claude");
    }

    #[test]
    fn extracts_title_summary_and_recent_requests_for_a_handoff() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let path = temporary_directory.path().join("session.jsonl");
        let session_id = "ffdf9b8d-61fe-4137-bdd0-2755c57796af";
        fs::write(
            &path,
            format!(
                concat!(
                    "{{\"type\":\"user\",\"sessionId\":\"{session_id}\",\"cwd\":\"/work/example\",\"message\":{{\"content\":\"First request\"}}}}\n",
                    "{{\"type\":\"ai-title\",\"sessionId\":\"{session_id}\",\"aiTitle\":\"Fix login flow\"}}\n",
                    "{{\"type\":\"summary\",\"summary\":\"Login work is half complete.\"}}\n",
                    "{{\"type\":\"user\",\"sessionId\":\"{session_id}\",\"message\":{{\"content\":\"Add regression tests\"}}}}\n"
                ),
                session_id = session_id,
            ),
        )
        .unwrap();

        let handoff = read_handoff_context(&path, session_id)
            .unwrap()
            .expect("handoff should be available");

        assert_eq!(handoff.title.as_deref(), Some("Fix login flow"));
        assert_eq!(
            handoff.summary.as_deref(),
            Some("Login work is half complete.")
        );
        assert_eq!(
            handoff.recent_user_requests,
            ["First request", "Add regression tests"]
        );
    }
}
