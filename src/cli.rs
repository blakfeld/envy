use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands;
use crate::config::DEFAULT_PROFILE;

#[derive(Parser)]
#[command(
    name = "devy",
    about = "Manage developer environments declaratively",
    allow_external_subcommands = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Set up the development environment defined in devy.yml
    Up {
        /// Re-resolve all versions and rewrite devy.lock
        #[arg(long)]
        update: bool,
        /// Validate without making changes (same as `envy check`)
        #[arg(long)]
        dry_run: bool,
        /// Only install dependencies tagged with this profile
        #[arg(long, default_value = DEFAULT_PROFILE)]
        profile: String,
    },
    /// Create an empty devy.yml in the current directory
    Init {
        /// Overwrite an existing devy.yml
        #[arg(long)]
        force: bool,
    },
    /// List services from devy.yml and their current running status
    Services {
        /// Filter by profile
        #[arg(long, default_value = DEFAULT_PROFILE)]
        profile: String,
    },
    /// Start a named service
    Start {
        /// Service name as defined in devy.yml
        name: String,
        #[arg(long, default_value = DEFAULT_PROFILE)]
        profile: String,
    },
    /// Stop a named service
    Stop {
        /// Service name as defined in devy.yml
        name: String,
        #[arg(long, default_value = DEFAULT_PROFILE)]
        profile: String,
    },
    /// Restart a named service (stop then start)
    Restart {
        /// Service name as defined in devy.yml
        name: String,
        #[arg(long, default_value = DEFAULT_PROFILE)]
        profile: String,
    },
    /// Stop all services defined in devy.yml
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
    /// Validate the environment matches devy.yml without making changes
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
    /// List commands from devy.yml — used by shell completion, not intended for direct use
    #[command(hide = true, name = "_commands")]
    #[allow(clippy::enum_variant_names)]
    ListCommands,
    /// Run a command defined in devy.yml
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
            Commands::Init { force } => {
                commands::init::run(*force, std::path::Path::new("devy.yml"))
            }
            Commands::Services { profile } => commands::service::list(profile),
            Commands::Start { name, profile } => commands::service::start(name, profile),
            Commands::Stop { name, profile } => commands::service::stop(name, profile),
            Commands::Restart { name, profile } => commands::service::restart(name, profile),
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::sync::Mutex;

    static CD_LOCK: Mutex<()> = Mutex::new(());

    fn with_tempdir<F: FnOnce() -> R, R>(f: F) -> R {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "devy_cli_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let _guard = CD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let orig = std::env::current_dir().ok();
        std::env::set_current_dir(&dir).unwrap();
        let result = f();
        if let Some(o) = orig {
            let _ = std::env::set_current_dir(o);
        }
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn cli_run_check_returns_err_without_config() {
        // Kills `replace Cli::run -> Ok(())` — mutation always returns Ok.
        // From a temp dir with no devy.yml, `check` must fail.
        let result = with_tempdir(|| {
            let cli = Cli::parse_from(["devy", "check"]);
            cli.run()
        });
        assert!(
            result.is_err(),
            "check must return Err when no devy.yml exists"
        );
    }

    #[test]
    fn cli_run_hook_returns_ok_for_valid_shell() {
        // Verifies that a successful command actually routes correctly.
        let cli = Cli::parse_from(["devy", "hook", "zsh"]);
        assert!(cli.run().is_ok(), "hook zsh must return Ok");
    }
}
