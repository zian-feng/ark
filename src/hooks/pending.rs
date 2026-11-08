use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::providers::discovery::{CachedFile, same_directory};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSession {
    pub provider: String,
    pub cwd: PathBuf,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub next_attempt_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub conflicted: bool,
    #[serde(default)]
    pub cache: BTreeMap<PathBuf, CachedFile>,
    #[serde(default)]
    pub last_error: Option<String>,
}

pub fn append(pid: u32, provider: &str, cwd: PathBuf) -> Result<()> {
    let path = path_for_pid(pid)?;
    append_at(&path, provider, cwd, Utc::now())
}

fn append_at(path: &Path, provider: &str, cwd: PathBuf, now: DateTime<Utc>) -> Result<()> {
    let mut sessions = load_at(path)?;
    let mut session = PendingSession::new(provider, cwd, now);
    // Mark both hooked invocations before either exits. File timestamps cannot
    // establish which of two simultaneous terminals owns a provider session.
    if let Some(parent) = path.parent()
        && parent.exists()
    {
        for entry in fs::read_dir(parent)? {
            let other_path = entry?.path();
            if other_path == path || other_path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let mut others = load_at(&other_path)?;
            let mut changed = false;
            for other in &mut others {
                if other.ended_at.is_none()
                    && other.provider == provider
                    && same_directory(&other.cwd, &session.cwd)
                {
                    other.conflicted = true;
                    session.conflicted = true;
                    changed = true;
                }
            }
            if changed {
                save_at(&other_path, &others)?;
            }
        }
    }
    sessions.push(session);
    save_at(path, &sessions)
}

impl PendingSession {
    pub fn new(provider: &str, cwd: PathBuf, started_at: DateTime<Utc>) -> Self {
        Self {
            provider: provider.to_owned(),
            cwd,
            started_at,
            ended_at: None,
            next_attempt_at: None,
            attempts: 0,
            conflicted: false,
            cache: BTreeMap::new(),
            last_error: None,
        }
    }

    pub fn expired(&self, now: DateTime<Utc>) -> bool {
        self.ended_at
            .is_some_and(|end| now - end >= chrono::Duration::minutes(5))
    }
}

pub fn load(pid: u32) -> Result<Vec<PendingSession>> {
    load_at(&path_for_pid(pid)?)
}

pub fn replace(pid: u32, sessions: &[PendingSession]) -> Result<()> {
    save_at(&path_for_pid(pid)?, sessions)
}

fn load_at(path: &Path) -> Result<Vec<PendingSession>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("could not read Ark pending session: {}", path.display())
            });
        }
    };

    if let Ok(sessions) = serde_json::from_str::<Vec<PendingSession>>(&contents) {
        return Ok(sessions);
    }

    // v0.1 initially stored one object per shell. Preserve any record written
    // before the queue format was introduced rather than losing an autosave.
    serde_json::from_str::<PendingSession>(&contents)
        .map(|session| vec![session])
        .with_context(|| format!("could not parse Ark pending session: {}", path.display()))
}

fn save_at(path: &Path, sessions: &[PendingSession]) -> Result<()> {
    if sessions.is_empty() {
        return match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| {
                format!("could not clear Ark pending session: {}", path.display())
            }),
        };
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "could not create Ark pending directory: {}",
                parent.display()
            )
        })?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        fs::write(&temporary, serde_json::to_string(sessions)?)?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("could not write Ark pending session: {}", path.display()))
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
    use chrono::Utc;

    use super::{PendingSession, append_at, load_at, save_at};

    #[test]
    fn retry_expiry_starts_at_exit_not_launch() {
        let now = Utc::now();
        let mut entry =
            PendingSession::new("codex", "/repo".into(), now - chrono::Duration::hours(3));
        assert!(!entry.expired(now));
        entry.ended_at = Some(now);
        assert!(!entry.expired(now + chrono::Duration::minutes(4)));
        assert!(entry.expired(now + chrono::Duration::minutes(5)));
    }

    #[test]
    fn overlapping_terminals_are_marked_but_finished_commands_are_not() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let a = directory.path().join("1.json");
        let b = directory.path().join("2.json");
        let now = Utc::now();
        append_at(&a, "codex", "/repo".into(), now)?;
        append_at(&b, "codex", "/repo".into(), now)?;
        assert!(load_at(&a)?[0].conflicted);
        assert!(load_at(&b)?[0].conflicted);
        for path in [&a, &b] {
            let mut entries = load_at(path)?;
            entries[0].ended_at = Some(now);
            save_at(path, &entries)?;
        }
        let c = directory.path().join("3.json");
        append_at(&c, "codex", "/repo".into(), now)?;
        assert!(!load_at(&c)?[0].conflicted);
        Ok(())
    }

    #[test]
    fn pending_queue_round_trips_and_reads_the_legacy_single_record() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("pending.json");
        let session = PendingSession::new("codex", "/work/example".into(), Utc::now());

        save_at(&path, std::slice::from_ref(&session))?;
        assert_eq!(load_at(&path)?.len(), 1);

        std::fs::write(
            &path,
            serde_json::json!({
                "provider": "codex", "cwd": "/work/example", "started_at": session.started_at
            })
            .to_string(),
        )?;
        assert_eq!(load_at(&path)?.len(), 1);
        assert!(load_at(&path)?[0].ended_at.is_none());

        save_at(&path, &[])?;
        assert!(load_at(&path)?.is_empty());
        Ok(())
    }
}
