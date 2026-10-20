use crate::{
    providers,
    slug::slugify,
    storage::{Database, SessionChanges},
};

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

pub fn update_session(
    key: &str,
    alias: Option<&str>,
    description: Option<&str>,
    provider_name: Option<&str>,
    cwd: Option<&Path>,
) -> Result<()> {
    if alias.is_none() && description.is_none() && provider_name.is_none() && cwd.is_none() {
        bail!("provide at least one field to update");
    }
    if provider_name.is_some() && cwd.is_some() {
        bail!("use either `--provider` or `--cwd`, not both");
    }

    let mut database = Database::open()?;
    let target = database.get_resume_session(key)?;

    let alias = match alias {
        Some(alias) => {
            let alias = slugify(alias);
            if alias.is_empty() {
                bail!("alias must contain at least one letter or number");
            }
            Some(alias)
        }
        None => None,
    };

    let (provider, cwd) = match provider_name {
        Some(provider_name) => {
            let provider = providers::get_provider(provider_name)
                .with_context(|| format!("unknown provider `{provider_name}`"))?;
            if !provider.is_available() {
                bail!("provider `{}` is not available on PATH", provider.name());
            }

            let discovered = provider
                .find_session_by_id(&target.session_id)?
                .with_context(|| {
                    format!(
                        "provider `{}` could not find session ID `{}`",
                        provider.name(),
                        target.session_id
                    )
                })?;

            (Some(discovered.provider.to_owned()), Some(discovered.cwd))
        }
        None => match cwd {
            Some(cwd) => (Some(target.provider), Some(normalize_directory(cwd)?)),
            None => (None, None),
        },
    };

    database.update_session(
        key,
        SessionChanges {
            alias,
            description: description.map(|description| description.trim().to_owned()),
            provider,
            cwd,
        },
    )
}

fn normalize_directory(path: &Path) -> Result<PathBuf> {
    if !path.is_dir() {
        bail!(
            "working directory does not exist or is not a directory: {}",
            path.display()
        );
    }
    fs::canonicalize(path)
        .with_context(|| format!("could not resolve working directory: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::normalize_directory;

    #[test]
    fn normalizes_an_existing_directory_and_rejects_a_file() -> anyhow::Result<()> {
        let temporary_directory = tempfile::tempdir()?;
        let resolved = normalize_directory(temporary_directory.path())?;
        assert!(resolved.is_absolute());

        let file = temporary_directory.path().join("not-a-directory");
        std::fs::write(&file, "test")?;
        assert!(normalize_directory(&file).is_err());
        Ok(())
    }
}
