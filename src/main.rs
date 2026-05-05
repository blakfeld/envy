mod cli;
mod commands;
mod config;
mod env_manager;
mod lock;
mod modules;
mod output;
mod package_manager;
mod secrets;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.run()
}
