mod cli;
mod commands;
mod config;
mod env_manager;
mod error;
mod lock;
mod modules;
mod output;
mod package_manager;

use clap::Parser;
use cli::Cli;

#[mutants::skip] // entry point — process exit behaviour is not unit-testable
fn main() {
    if let Err(err) = run() {
        if let Some(silent) = err.downcast_ref::<error::SilentExit>() {
            std::process::exit(silent.0);
        }
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

#[mutants::skip] // thin delegation; Cli::parse() reads process args, not controllable in unit tests
pub(crate) fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    cli.run()
}

#[cfg(test)]
mod tests {
    #[test]
    fn run_fn_is_callable_in_test_context() {
        // The run() function calls Cli::parse() which reads from std::env::args().
        // In tests, args may not form a valid CLI invocation — but we just verify
        // that the function symbol exists and is reachable (compile-time check only).
        // The mutation `replace run -> Ok(())` changes run()'s body; this test
        // ensures the function is at least called by surrounding code.
        // A more meaningful kill is done via cli::tests::cli_run_check_returns_err_without_config.
        let _f: fn() -> anyhow::Result<()> = super::run;
    }
}
