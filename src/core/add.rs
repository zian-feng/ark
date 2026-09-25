use crate::{
    providers,
    slug::slugify,
    storage::{Database, NewSession, Session},
};

use anyhow::{Context, Result, bail};

pub fn add_new_session(
    session_id: &str,
    alias: Option<&str>,
    provider_name: Option<&str>,
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
        description: None,
        tags: None,
        starred: false,
    })
}
