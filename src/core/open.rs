use crate::{providers, storage::Database};
use anyhow::{Context, Result, bail};

pub fn open_session(key: &str) -> Result<()> {
    let database = Database::open()?;
    let target = database.get_resume_session(key)?;
    let provider = providers::get_provider(&target.provider)
        .with_context(|| format!("unknown provider `{}`", target.provider))?;

    if !provider.is_available() {
        bail!("provider `{}` is not available on PATH", provider.name());
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
