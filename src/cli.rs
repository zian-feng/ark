use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use crate::core;

const DESCRIPTION_DISPLAY_LENGTH: usize = 80;

#[derive(Parser)]
#[command(name = "ark", about = "Save and resume AI coding sessions")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Resume a saved session by its Ark alias or native session ID.
    #[arg(value_name = "ALIAS")]
    alias: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// add a session to ark using native session ID.
    Add {
        session_id: String,
        alias: Option<String>,

        /// Provider that owns the native session ID.
        #[arg(long)]
        provider: Option<String>,

        /// Description to save instead of an automatic session description.
        #[arg(long = "desc", visible_alias = "description")]
        description: Option<String>,
    },

    /// Remove a saved session from Ark by its alias or native session ID.
    Rm { key: String },

    /// Star a session by alias or native session ID.
    Star { key: String },

    /// Remove a session's star by alias or native session ID.
    Unstar { key: String },

    /// list all saved sessions.
    List,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let Cli { command, alias } = self;

        match (command, alias) {
            (
                Some(Command::Add {
                    session_id,
                    alias,
                    provider,
                    description,
                }),
                None,
            ) => {
                let session = core::add::add_new_session(
                    &session_id,
                    alias.as_deref(),
                    provider.as_deref(),
                    description.as_deref(),
                )?;
                println!("Added `{}` ({} session)", session.id, session.provider);
            }

            (Some(Command::Rm { key }), None) => {
                core::remove::remove_session(&key)?;
                println!("Removed `{key}` from Ark.");
            }

            (Some(Command::Star { key }), None) => {
                core::star::set_starred(&key, true)?;
                println!("Starred `{key}`.");
            }

            (Some(Command::Unstar { key }), None) => {
                core::star::set_starred(&key, false)?;
                println!("Unstarred `{key}`.");
            }

            (Some(Command::List), None) | (None, None) => {
                let sessions = core::list::list_sessions()?;

                if sessions.is_empty() {
                    println!("No saved sessions.");
                    return Ok(());
                }

                println!(
                    "     {:<25}  {:<38}  {:<10}  DESCRIPTION",
                    "ALIAS", "SESSION ID", "PROVIDER"
                );

                for session in sessions {
                    let starred = if session.starred { "  *  " } else { "     " };
                    let alias = if session.id == session.session_id {
                        ""
                    } else {
                        &session.id
                    };
                    let description = truncate_description_for_display(&session.description);

                    println!(
                        "{}{:<25}  {:<38}  {:<10}  {}",
                        starred, alias, session.session_id, session.provider, description
                    );
                }
            }

            (None, Some(alias)) => {
                core::open::open_session(&alias)?;
            }

            (_, Some(_)) => {
                bail!("an alias cannot be combined with a subcommand");
            }
        }

        Ok(())
    }
}

fn truncate_description_for_display(description: &str) -> String {
    if description.chars().count() <= DESCRIPTION_DISPLAY_LENGTH {
        return description.to_owned();
    }

    let shortened = description
        .chars()
        .take(DESCRIPTION_DISPLAY_LENGTH - 1)
        .collect::<String>();
    format!("{shortened}…")
}

#[cfg(test)]
mod tests {
    use super::truncate_description_for_display;

    #[test]
    fn truncates_long_descriptions_for_list_display() {
        let description = "x".repeat(81);
        let displayed = truncate_description_for_display(&description);

        assert_eq!(displayed.chars().count(), 80);
        assert!(displayed.ends_with('…'));
    }
}
