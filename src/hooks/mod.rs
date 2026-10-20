mod pending;
pub mod zsh;

use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
    thread,
    time::Duration,
};

use anyhow::Result;

use crate::{config::Config, core, providers};

pub fn pre(provider: &str, cwd: PathBuf, pid: u32) -> Result<()> {
    if !Config::load()?.auto_mode {
        return Ok(());
    }
    pending::write(pid, provider, cwd)
}

pub fn post(pid: u32) -> Result<()> {
    let config = Config::load()?;
    if !config.auto_mode {
        return Ok(());
    }
    let Some(pending) = pending::take(pid)? else {
        return Ok(());
    };
    let Some(provider) = providers::get_provider(&pending.provider) else {
        return Ok(());
    };

    // Provider files can be flushed just after the CLI process exits.
    for attempt in 0..3 {
        match provider.find_recent_session(&pending.cwd, pending.started_at) {
            Ok(Some(session)) => {
                if config.auto_confirm && !confirm(&session)? {
                    return Ok(());
                }
                if let Some(saved) = core::add::add_discovered_session(session)? {
                    println!("Ark: saved `{}` ({})", saved.id, saved.provider);
                }
                return Ok(());
            }
            Ok(None) | Err(_) if attempt < 2 => thread::sleep(Duration::from_millis(200)),
            Ok(None) | Err(_) => return Ok(()),
        }
    }
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
