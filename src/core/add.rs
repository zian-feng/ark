use crate::{
    providers,
    slug::slugify,
    storage::{Database, MAX_DESCRIPTION_LENGTH, NewSession, Session},
};

use anyhow::{Context, Result, bail};

pub fn add_new_session(
    session_id: &str,
    alias: Option<&str>,
    provider_name: Option<&str>,
    user_description: Option<&str>,
) -> Result<Session> {
    let database = Database::open()?;
    let discovered = match provider_name {
        Some(provider_name) => {
            let provider = providers::get_provider(provider_name)
                .with_context(|| format!("unknown provider `{provider_name}`"))?;

            let session = match provider.find_session_by_id(session_id)? {
                Some(session) => session,
                None => {
                    let matches =
                        providers::find_session_in_other_providers(session_id, provider.name())?;

                    match matches.as_slice() {
                        [session] => bail!(
                            "session ID `{session_id}` was not found in provider `{}`, but it was found in `{}`; retry with `--provider {}` or omit --provider to auto-detect it",
                            provider.name(),
                            session.provider,
                            session.provider,
                        ),
                        [] => bail!(
                            "provider `{}` could not find session ID `{session_id}`",
                            provider.name()
                        ),
                        sessions => {
                            let names = sessions
                                .iter()
                                .map(|session| session.provider)
                                .collect::<Vec<_>>()
                                .join(", ");
                            bail!(
                                "session ID `{session_id}` was not found in provider `{}`, but it was found in multiple other providers ({names}); choose one with --provider",
                                provider.name()
                            )
                        }
                    }
                }
            };

            session
        }
        None => providers::auto_detect_provider(session_id)?,
    };

    let provider = providers::get_provider(discovered.provider)
        .with_context(|| format!("unknown provider `{}`", discovered.provider))?;
    if !provider.is_available() {
        bail!("provider `{}` is not available on PATH", provider.name());
    }

    let description = choose_description(
        user_description,
        discovered.preview.as_deref(),
        provider.name(),
        session_id,
    )?;

    let id = match alias {
        Some(alias) => {
            let id = slugify(alias);

            if id.is_empty() {
                bail!("alias must contain at least one letter or number")
            }

            id
        }

        None => session_id.to_owned(),
    };

    database.add_session(NewSession {
        id,
        session_id: session_id.to_owned(),
        provider: discovered.provider.to_owned(),
        cwd: discovered.cwd,
        description: Some(description),
        tags: None,
        starred: false,
    })
}

fn choose_description(
    user_description: Option<&str>,
    automatic_description: Option<&str>,
    provider: &str,
    session_id: &str,
) -> Result<String> {
    if let Some(description) = user_description
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if description.chars().count() > MAX_DESCRIPTION_LENGTH {
            bail!("description must be at most {MAX_DESCRIPTION_LENGTH} characters");
        }
        return Ok(description.to_owned());
    }

    Ok(automatic_description
        .and_then(normalize_automatic_description)
        .unwrap_or_else(|| {
            format!(
                "{provider} session {}",
                &session_id[..session_id.len().min(8)]
            )
        }))
}

fn normalize_automatic_description(description: &str) -> Option<String> {
    let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
    if description.is_empty() {
        return None;
    }

    if description.chars().count() <= MAX_DESCRIPTION_LENGTH {
        return Some(description);
    }

    let shortened = description
        .chars()
        .take(MAX_DESCRIPTION_LENGTH - 1)
        .collect::<String>();
    Some(format!("{shortened}…"))
}

#[cfg(test)]
mod tests {
    use crate::storage::MAX_DESCRIPTION_LENGTH;

    use super::choose_description;

    #[test]
    fn user_description_overrides_the_automatic_description() {
        assert_eq!(
            choose_description(
                Some("My own description"),
                Some("First Codex prompt"),
                "codex",
                "019bc371-82cf-7d82-ad0b-96d026aaca73"
            )
            .unwrap(),
            "My own description"
        );
    }

    #[test]
    fn automatic_description_uses_a_normalized_prompt() {
        assert_eq!(
            choose_description(
                None,
                Some("Add  \n  rate limiting"),
                "codex",
                "019bc371-82cf-7d82-ad0b-96d026aaca73"
            )
            .unwrap(),
            "Add rate limiting"
        );
    }

    #[test]
    fn missing_descriptions_use_a_provider_fallback() {
        assert_eq!(
            choose_description(None, None, "codex", "019bc371-82cf-7d82-ad0b-96d026aaca73")
                .unwrap(),
            "codex session 019bc371"
        );
    }

    #[test]
    fn rejects_an_overlong_user_description() {
        let error = choose_description(
            Some(&"x".repeat(MAX_DESCRIPTION_LENGTH + 1)),
            None,
            "codex",
            "019bc371-82cf-7d82-ad0b-96d026aaca73",
        )
        .expect_err("an overlong description should be rejected");

        assert!(error.to_string().contains("at most"));
    }
}
