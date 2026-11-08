mod pending;
pub mod zsh;

use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};

use crate::{config::Config, core, providers};

pub fn pre(provider: &str, cwd: PathBuf, pid: u32) -> Result<()> {
    if !Config::load()?.auto_mode {
        return Ok(());
    }
    if providers::get_provider(provider).is_none() {
        anyhow::bail!("unknown provider `{provider}`");
    }
    pending::append(pid, provider, cwd)
}

pub fn post(pid: u32) -> Result<()> {
    let config = Config::load()?;
    if !config.auto_mode {
        return pending::replace(pid, &[]);
    }
    let pending_sessions = pending::load(pid)?;
    if pending_sessions.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    let mut pending_sessions = pending_sessions;
    // Persist the first post-exit time before discovery. A three-hour agent
    // session gets the same retry window as a three-second session.
    for entry in &mut pending_sessions {
        entry.ended_at.get_or_insert(now);
    }
    pending::replace(pid, &pending_sessions)?;
    let mut unresolved = Vec::new();
    for mut pending_session in pending_sessions {
        if pending_session.expired(now) {
            eprintln!(
                "Ark: auto-save expired for {} in {}: {}. Use `ark add <session-id>`.",
                pending_session.provider,
                pending_session.cwd.display(),
                pending_session
                    .last_error
                    .as_deref()
                    .unwrap_or("no matching session appeared")
            );
            continue;
        }
        if pending_session.conflicted {
            eprintln!(
                "Ark: overlapping {} commands in {}; cannot identify the owning terminal. Use `ark add <session-id>`.",
                pending_session.provider,
                pending_session.cwd.display()
            );
            continue;
        }
        if pending_session
            .next_attempt_at
            .is_some_and(|next| next > now)
        {
            unresolved.push(pending_session);
            continue;
        }
        let Some(provider) = providers::get_provider(&pending_session.provider) else {
            continue;
        };

        let end = pending_session
            .ended_at
            .expect("post-exit time was set above");
        let scan = providers::discovery::scan(
            provider,
            &pending_session.cwd,
            pending_session.started_at,
            end,
            &mut pending_session.cache,
        );
        match scan {
            Ok(mut scan) => {
                if std::env::var_os("ARK_AUTO_DEBUG").is_some() {
                    eprintln!(
                        "Ark auto: {} candidate(s), {} changed file(s) parsed, {} error(s)",
                        scan.sessions.len(),
                        scan.parsed,
                        scan.errors.len()
                    );
                }
                if scan.sessions.len() > 1 {
                    eprintln!(
                        "Ark: multiple {} sessions match this command ({}); use `ark add <session-id>`.",
                        provider.name(),
                        scan.sessions
                            .iter()
                            .map(|s| s.session_id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    continue;
                }
                // A malformed candidate could hide another matching session.
                if scan.errors.is_empty()
                    && let Some(session) = scan.sessions.pop()
                {
                    if config.auto_confirm {
                        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                            unresolved.push(pending_session);
                            continue;
                        }
                        if !confirm(&session)? {
                            continue;
                        }
                    }
                    match core::add::add_discovered_session(session) {
                        Ok(saved) => {
                            if let Some(saved) = saved {
                                println!("Ark: saved `{}` ({})", saved.id, saved.provider);
                            }
                            continue;
                        }
                        Err(error) => pending_session.last_error = Some(format!("{error:#}")),
                    }
                } else {
                    pending_session.last_error = Some(
                        scan.errors
                            .into_iter()
                            .next()
                            .unwrap_or_else(|| "no matching new session appeared".to_owned()),
                    );
                }
            }
            Err(error) => pending_session.last_error = Some(format!("{error:#}")),
        }
        if std::env::var_os("ARK_AUTO_DEBUG").is_some() {
            eprintln!(
                "Ark auto: retry pending: {}",
                pending_session.last_error.as_deref().unwrap_or("unknown")
            );
        }
        pending_session.attempts = pending_session.attempts.saturating_add(1);
        let delay = [2, 5, 10, 30][pending_session.attempts.saturating_sub(1).min(3) as usize];
        pending_session.next_attempt_at = Some(now + ChronoDuration::seconds(delay));
        unresolved.push(pending_session);
    }

    pending::replace(pid, &unresolved)?;
    Ok(())
}

fn confirm(session: &providers::DiscoveredSession) -> Result<bool> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Ok(false);
    }
    print!(
        "Ark detected a new {} session. Save it? [Y/n] ",
        session.provider
    );
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(!matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "n" | "no"
    ))
}
