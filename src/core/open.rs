use crate::{
    providers,
    storage::{Database, SessionChanges},
};
use anyhow::{Context, Result, bail};

pub fn open_session(key: &str) -> Result<()> {
    let mut database = Database::open()?;
    let mut target = database.get_resume_session(key)?;
    let provider = providers::get_provider(&target.provider)
        .with_context(|| format!("unknown provider `{}`", target.provider))?;

    if !provider.is_available() {
        bail!("provider `{}` is not available on PATH", provider.name());
    }

    // TODO: Codex can prompt to use either its historical session directory or
    // the caller's current directory. When resume cwd policy becomes
    // provider-specific, keep this recovery path for Claude but let Codex use
    // its native prompt instead of forcing the saved cwd.
    if !target.cwd.is_dir() {
        if let Some(discovered) = provider.find_session_by_id(&target.session_id)?
            && discovered.cwd.is_dir()
            && discovered.cwd != target.cwd
        {
            database.update_session(
                key,
                SessionChanges {
                    alias: None,
                    description: None,
                    provider: Some(discovered.provider.to_owned()),
                    cwd: Some(discovered.cwd.clone()),
                },
            )?;
            target.cwd = discovered.cwd;
        } else {
            bail!(
                "cannot open `{key}`: its saved project directory no longer exists: {}\n\nUpdate it with:\n  ark update {key} --cwd /new/path",
                target.cwd.display()
            );
        }
    }

    let mut command = provider.resume_command(&target.session_id);
    command.current_dir(&target.cwd);
    let exit_status = command
        .status()
        .with_context(|| format!("could not launch provider `{}`", provider.name()))?;

    if !exit_status.success() {
        bail!(
            "provider `{}` exited with status {exit_status}",
            provider.name()
        );
    }

    Ok(())
}
