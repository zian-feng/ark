use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use crate::core;

#[derive(Parser)]
#[command(name = "ark", about = "Save and resume AI coding sessions")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// resume a saved session by session_id
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// add a session to ark using native session ID.
    Add { session_id: String },

    /// removes a saved session from ark
    Rm { session_id: String },

    /// list all saved sessions.
    List,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let Cli {
            command,
            session_id,
        } = self;

        match (command, session_id) {
            (Some(Command::Add { session_id }), None) => {
                let session = core::add::add_new_session_codex(&session_id)?;
                println!("Added `{}` ({} session)", session.id, session.provider);
            }

            (Some(Command::Rm { session_id }), None) => {
                core::remove::remove_session(&session_id)?;
                println!("Removed `{session_id}` from Ark.");
            }

            (Some(Command::List), None) | (None, None) => {
                let sessions = core::list::list_sessions()?;

                if sessions.is_empty() {
                    println!("No saved sessions.");
                    return Ok(());
                }

                println!(
                    "{:<8}  {:<38}  {:<10}  DESCRIPTION",
                    "STARRED", "SESSION ID", "PROVIDER"
                );

                for session in sessions {
                    let starred = if session.starred { "*" } else { "" };

                    println!(
                        "{:<8}  {:<38}  {:<10}  {}",
                        starred, session.session_id, session.provider, session.description
                    );
                }
            }

            (None, Some(session_id)) => {
                core::open::open_session(&session_id)?;
            }

            (_, Some(_)) => {
                bail!("a session ID cannot be combined with a subcommand");
            }
        }

        Ok(())
    }
}
