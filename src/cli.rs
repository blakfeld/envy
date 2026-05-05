use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands;
use crate::config::DEFAULT_PROFILE;

#[derive(Parser)]
#[command(
    name = "envy",
    about = "Manage developer environments declaratively",
    allow_external_subcommands = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Set up the development environment defined in envy.yml
    Up {
        /// Re-resolve all versions and rewrite envy.lock
        #[arg(long)]
        update: bool,
        /// Validate without making changes (same as `envy check`)
        #[arg(long)]
        dry_run: bool,
        /// Only install dependencies tagged with this profile
        #[arg(long, default_value = DEFAULT_PROFILE)]
        profile: String,
    },
    /// Scaffold an envy.yml by detecting the current project's languages and tools
    Init {
        /// Overwrite an existing envy.yml
        #[arg(long)]
        force: bool,
    },
    /// Stop all services defined in envy.yml
    Down {
        /// Only stop services tagged with this profile
        #[arg(long, default_value = DEFAULT_PROFILE)]
        profile: String,
    },
    /// Show install, service, and environment status
    Status {
        /// Filter by profile
        #[arg(long, default_value = DEFAULT_PROFILE)]
        profile: String,
    },
    /// Validate the environment matches envy.yml without making changes
    Check {
        /// Filter by profile
        #[arg(long, default_value = DEFAULT_PROFILE)]
        profile: String,
    },
    /// Print a shell integration snippet to eval in your rc file
    Hook {
        /// Shell to generate the snippet for (zsh, bash, fish)
        shell: String,
    },
    /// List commands from envy.yml — used by shell completion, not intended for direct use
    #[command(hide = true, name = "_commands")]
    #[allow(clippy::enum_variant_names)]
    ListCommands,
    /// Run a command defined in envy.yml
    #[command(external_subcommand)]
    External(Vec<String>),
}

impl Cli {
    pub fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Up {
                update: _,
                dry_run: true,
                profile,
            } => commands::check::run(profile),
            Commands::Up {
                update,
                dry_run: false,
                profile,
            } => commands::up::run(*update, profile),
            Commands::Init { force } => commands::init::run(*force),
            Commands::Down { profile } => commands::down::run(profile),
            Commands::Status { profile } => commands::status::run(profile),
            Commands::Check { profile } => commands::check::run(profile),
            Commands::Hook { shell } => commands::hook::run(shell),
            Commands::ListCommands => {
                commands::list_commands::run();
                Ok(())
            }
            Commands::External(args) => commands::exec::run(&args[0]),
        }
    }
}
