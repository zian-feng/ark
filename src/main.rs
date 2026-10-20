use anyhow::Result;
use clap::Parser;

mod cli;
mod config;
mod core;
mod hooks;
mod providers;
mod slug;
mod storage;
mod tui;

fn main() -> Result<()> {
    cli::Cli::parse().run()
}
