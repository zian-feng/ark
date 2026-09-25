use anyhow::Result;
use clap::Parser;

mod cli;
mod core;
mod providers;
mod slug;
mod storage;

fn main() -> Result<()> {
    cli::Cli::parse().run()
}
