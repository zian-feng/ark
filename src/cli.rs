use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use std::{io::IsTerminal, path::PathBuf};

use crate::{config::Config, core, hooks, tui};

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

    /// Start a new provider session with context from a saved session.
    Fwd {
        /// Source Ark alias or native session ID.
        key: String,

        /// Provider for the new session, such as `claude` or `codex`.
        target_provider: String,
    },

    /// Update a saved session's metadata.
    Update {
        key: String,

        /// New Ark alias for the session.
        #[arg(long)]
        alias: Option<String>,

        /// New session description.
        #[arg(long = "desc", visible_alias = "description")]
        description: Option<String>,

        /// Provider that owns the native session ID.
        #[arg(long)]
        provider: Option<String>,

        /// New working directory for the session, for example after moving a repository.
        #[arg(long)]
        cwd: Option<PathBuf>,
    },

    /// list all saved sessions.
    #[command(visible_alias = "ls")]
    List,

    /// Configure automatic saving after an interactive provider session ends.
    Auto {
        #[arg(value_enum)]
        mode: Option<AutoMode>,

        /// Ask before saving each detected session (requires `on`).
        #[arg(long)]
        confirm: bool,
    },

    /// Print shell integration code.
    Init {
        #[arg(value_enum)]
        shell: Shell,
    },

    #[command(hide = true)]
    Hook {
        #[command(subcommand)]
        command: HookCommand,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum AutoMode {
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
enum Shell {
    Zsh,
}

#[derive(Subcommand)]
enum HookCommand {
    Pre {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        pid: u32,
    },
    Post {
        #[arg(long)]
        pid: u32,
    },
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

            (
                Some(Command::Fwd {
                    key,
                    target_provider,
                }),
                None,
            ) => match core::forward::forward_session(&key, &target_provider)? {
                Some(session) => println!(
                    "Ark: forwarded `{key}` to {} as `{}`.",
                    session.provider, session.id
                ),
                None => println!(
                    "Ark: forwarded `{key}` to `{target_provider}`, but could not locate its new session afterward."
                ),
            },

            (
                Some(Command::Update {
                    key,
                    alias,
                    description,
                    provider,
                    cwd,
                }),
                None,
            ) => {
                core::update::update_session(
                    &key,
                    alias.as_deref(),
                    description.as_deref(),
                    provider.as_deref(),
                    cwd.as_deref(),
                )?;
                println!("Updated `{key}`.");
            }

            (Some(Command::List), None) => {
                print_list()?;
            }

            (Some(Command::Auto { mode, confirm }), None) => {
                configure_auto(mode, confirm)?;
            }

            (Some(Command::Init { shell: Shell::Zsh }), None) => {
                print!("{}", hooks::zsh::script());
            }

            (
                Some(Command::Hook {
                    command: HookCommand::Pre { provider, cwd, pid },
                }),
                None,
            ) => hooks::pre(&provider, cwd, pid)?,

            (
                Some(Command::Hook {
                    command: HookCommand::Post { pid },
                }),
                None,
            ) => {
                hooks::post(pid)?;
            }

            (None, None) if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() => {
                if let Some(key) = tui::run()? {
                    core::open::open_session(&key)?;
                }
            }

            (None, None) => {
                print_list()?;
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

fn configure_auto(mode: Option<AutoMode>, confirm: bool) -> Result<()> {
    let mut config = Config::load()?;
    match mode {
        Some(AutoMode::On) => {
            config.auto_mode = true;
            config.auto_confirm = confirm;
            config.save()?;
            println!(
                "Ark auto mode is on{}. Add `eval \"$(ark init zsh)\"` to ~/.zshrc if you have not installed the hook.",
                if confirm {
                    " (confirmation required)"
                } else {
                    ""
                }
            );
        }
        Some(AutoMode::Off) => {
            if confirm {
                bail!("`--confirm` can only be used with `ark auto on`");
            }
            config.auto_mode = false;
            config.auto_confirm = false;
            config.save()?;
            println!("Ark auto mode is off.");
        }
        None => println!(
            "Ark auto mode is {}{}.",
            if config.auto_mode { "on" } else { "off" },
            if config.auto_mode && config.auto_confirm {
                " (confirmation required)"
            } else {
                ""
            }
        ),
    }
    Ok(())
}

fn print_list() -> Result<()> {
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

    Ok(())
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
