mod claude;
mod codex;

use std::{env, path::PathBuf, process::Command};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIdHint {
    UuidV4,
    UuidV7,
    Other,
}

#[derive(Debug, Clone)]
pub struct DiscoveredSession {
    pub session_id: String,
    pub provider: &'static str,
    pub cwd: PathBuf,
    pub modified_at: DateTime<Utc>,
    pub preview: Option<String>,
    pub file_path: PathBuf,
}

pub trait Provider {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn id_hint_priority(&self, hint: SessionIdHint) -> u8;
    fn find_session_by_id(&self, session_id: &str) -> Result<Option<DiscoveredSession>>;
    fn resume_command(&self, session_id: &str) -> Command;
}

static CLAUDE: claude::ClaudeProvider = claude::ClaudeProvider;
static CODEX: codex::CodexProvider = codex::CodexProvider;

fn all_providers() -> [&'static dyn Provider; 2] {
    [&CLAUDE, &CODEX]
}

pub fn get_provider(name: &str) -> Option<&'static dyn Provider> {
    match name {
        "claude" => Some(&CLAUDE),
        "codex" => Some(&CODEX),
        _ => None,
    }
}

pub fn auto_detect_provider(session_id: &str) -> Result<DiscoveredSession> {
    let hint = session_id_hint(session_id);
    let mut providers = all_providers().to_vec();
    providers.sort_by_key(|provider| provider.id_hint_priority(hint));

    let mut matches = Vec::new();
    for provider in providers {
        if let Some(session) = provider.find_session_by_id(session_id)? {
            matches.push(session);
        }
    }

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => {
            bail!("could not auto-detect a provider for session ID `{session_id}`; use --provider")
        }
        _ => {
            let names = matches
                .iter()
                .map(|session| session.provider)
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "session ID `{session_id}` was found in multiple providers ({names}); use --provider"
            )
        }
    }
}

pub fn session_id_hint(session_id: &str) -> SessionIdHint {
    let bytes = session_id.as_bytes();

    if bytes.len() != 36
        || [8, 13, 18, 23].iter().any(|&index| bytes[index] != b'-')
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| ![8, 13, 18, 23].contains(&index) && !byte.is_ascii_hexdigit())
    {
        return SessionIdHint::Other;
    }

    match bytes[14].to_ascii_lowercase() {
        b'4' => SessionIdHint::UuidV4,
        b'7' => SessionIdHint::UuidV7,
        _ => SessionIdHint::Other,
    }
}

pub(crate) fn binary_on_path(binary: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|directory| directory.join(binary).is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::{SessionIdHint, get_provider, session_id_hint};

    #[test]
    fn finds_the_claude_provider() {
        let provider = get_provider("claude").expect("Claude provider should be registered");
        assert_eq!(provider.name(), "claude");
    }

    #[test]
    fn finds_the_codex_provider() {
        let provider = get_provider("codex").expect("Codex provider should be registered");
        assert_eq!(provider.name(), "codex");
    }

    #[test]
    fn rejects_unknown_providers() {
        assert!(get_provider("unknown").is_none());
    }

    #[test]
    fn detects_uuid_version_hints() {
        assert_eq!(
            session_id_hint("550e8400-e29b-41d4-a716-446655440000"),
            SessionIdHint::UuidV4
        );
        assert_eq!(
            session_id_hint("019bc371-82cf-7d82-ad0b-96d026aaca73"),
            SessionIdHint::UuidV7
        );
        assert_eq!(session_id_hint("not-a-uuid"), SessionIdHint::Other);
    }
}
