use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use crate::core;

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
    },

    /// Remove a saved session from Ark by its alias or native session ID.
    Rm { key: String },

    /// list all saved sessions.
    List,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let Cli { command, alias } = self;

        match (command, alias) {
            (Some(Command::Add { session_id, alias }), None) => {
                let session = core::add::add_new_session_codex(&session_id, alias.as_deref())?;
                println!("Added `{}` ({} session)", session.id, session.provider);
            }

            (Some(Command::Rm { key }), None) => {
                core::remove::remove_session(&key)?;
                println!("Removed `{key}` from Ark.");
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

                    println!(
                        "{}{:<25}  {:<38}  {:<10}  {}",
                        starred, alias, session.session_id, session.provider, session.description
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
