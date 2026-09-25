use crate::{
    providers,
    slug::slugify,
    storage::{Database, SessionChanges},
};

use anyhow::{Context, Result, bail};

pub fn update_session(
    key: &str,
    alias: Option<&str>,
    description: Option<&str>,
    provider_name: Option<&str>,
) -> Result<()> {
    if alias.is_none() && description.is_none() && provider_name.is_none() {
        bail!("provide at least one field to update");
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
        None => (None, None),
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
