//! Bounded, per-pending-command discovery. This is not a persistent history index.
use super::{DiscoveredSession, Provider};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoSession {
    pub session_id: String,
    pub cwd: PathBuf,
    pub created_at: DateTime<Utc>,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFile {
    modified: DateTime<Utc>,
    length: u64,
    session: Option<AutoSession>,
    error: Option<String>,
}

#[derive(Default)]
pub struct Scan {
    pub sessions: Vec<DiscoveredSession>,
    pub errors: Vec<String>,
    pub parsed: usize,
}

pub fn same_directory(left: &Path, right: &Path) -> bool {
    left == right
        || matches!((fs::canonicalize(left), fs::canonicalize(right)), (Ok(a), Ok(b)) if a == b)
}

pub fn scan(
    provider: &dyn Provider,
    cwd: &Path,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    cache: &mut BTreeMap<PathBuf, CachedFile>,
) -> Result<Scan> {
    scan_files(
        provider.auto_session_files(cwd, start, end)?,
        provider,
        cwd,
        start,
        end,
        cache,
    )
}

fn scan_files(
    files: Vec<PathBuf>,
    provider: &dyn Provider,
    cwd: &Path,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    cache: &mut BTreeMap<PathBuf, CachedFile>,
) -> Result<Scan> {
    let mut result = Scan::default();
    for path in files {
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                result.errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let modified: DateTime<Utc> = metadata.modified()?.into();
        if modified < start {
            continue;
        }
        let unchanged = cache
            .get(&path)
            .is_some_and(|entry| entry.modified == modified && entry.length == metadata.len());
        if !unchanged {
            result.parsed += 1;
            let (session, error) = match provider.read_auto_session(&path) {
                Ok(session) => (session, None),
                Err(error) => (None, Some(format!("{}: {error:#}", path.display()))),
            };
            cache.insert(
                path.clone(),
                CachedFile {
                    modified,
                    length: metadata.len(),
                    session,
                    error,
                },
            );
        }
        let entry = &cache[&path];
        if let Some(error) = &entry.error {
            result.errors.push(error.clone());
        }
        if let Some(session) = &entry.session
            && session.created_at >= start
            && session.created_at <= end
            && same_directory(&session.cwd, cwd)
            && !result
                .sessions
                .iter()
                .any(|existing| existing.session_id == session.session_id)
        {
            result.sessions.push(DiscoveredSession {
                session_id: session.session_id.clone(),
                provider: provider.name(),
                cwd: session.cwd.clone(),
                modified_at: modified,
                preview: session.preview.clone(),
            });
        }
    }
    Ok(result)
}

/// Read at most 1 MiB, ignoring an incomplete trailing JSON record. Old history
/// and tool output beyond this budget cannot delay the shell prompt.
pub fn records(path: &Path) -> Result<Vec<Value>> {
    let mut reader = BufReader::new(File::open(path)?.take(1_048_576));
    let mut records = Vec::new();
    let mut line = Vec::new();
    while reader.read_until(b'\n', &mut line)? != 0 {
        match serde_json::from_slice(&line) {
            Ok(value) => records.push(value),
            Err(error) if !line.ends_with(b"\n") && (error.is_eof() || error.is_syntax()) => break,
            Err(error) => {
                return Err(error).with_context(|| format!("invalid JSONL in {}", path.display()));
            }
        }
        line.clear();
    }
    Ok(records)
}

pub fn jsonl_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry.path().extension().is_some_and(|ext| ext == "jsonl")
        {
            files.push(entry.path());
        }
    }
    Ok(files)
}
