use anyhow::{Context, Result, bail};
use std::process::Command;

use crate::config::{EnvyCommand, EnvyConfig, HookAction};
use crate::output;

fn spawn_cmd(cmd: &EnvyCommand, label: &str) -> Result<()> {
    let mut proc = Command::new(&cmd.shell);
    proc.arg("-c").arg(&cmd.cmd);
    if let Some(cwd) = &cmd.cwd {
        proc.current_dir(cwd);
    }
    let status = proc
        .status()
        .with_context(|| format!("Failed to spawn '{}' via {}", label, cmd.shell))?;
    if !status.success() {
        bail!(
            "'{}' exited with status {}",
            label,
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

/// Runs all commands in a hook. Aborts with an error if any command exits non-zero.
pub fn run_hook(label: &str, action: &HookAction) -> Result<()> {
    for raw in action.commands() {
        let cmd = EnvyCommand::from(raw.clone());
        output::step(&format!("Running hook '{}'", label));
        spawn_cmd(&cmd, label)?;
        output::success(&format!("Hook '{}' succeeded", label));
    }
    Ok(())
}

#[mutants::skip] // thin I/O wrapper — requires a real devy.yml on disk
pub fn run(name: &str) -> Result<()> {
    let config = EnvyConfig::load_default()?;

    let raw = config.commands.get(name).cloned();

    let cmd = match raw {
        Some(raw) => EnvyCommand::from(raw),
        None => {
            let mut available: Vec<&str> =
                config.commands.keys().map(|k| k.as_str()).collect();
            available.sort_unstable();
            if available.is_empty() {
                bail!("Unknown command '{}'. No commands are defined.", name);
            } else {
                bail!(
                    "Unknown command '{}'. Available: {}",
                    name,
                    available.join(", ")
                );
            }
        }
    };

    output::step(&format!("Running '{}'", name));
    spawn_cmd(&cmd, name)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CommandConfig, HookAction};

    #[test]
    fn run_hook_with_succeeding_command_returns_ok() {
        let action = HookAction::Single(RawCommand::Simple("true".into()));
        assert!(run_hook("test_hook", &action).is_ok());
    }

    #[test]
    fn run_hook_with_failing_command_returns_err() {
        let action = HookAction::Single(RawCommand::Simple("false".into()));
        let err = run_hook("test_hook", &action).unwrap_err();
        assert!(err.to_string().contains("test_hook"));
    }

    #[test]
    fn run_hook_with_custom_shell_uses_that_shell() {
        let action = HookAction::Single(RawCommand::Configured(CommandConfig {
            cmd: "true".into(),
            cwd: None,
            shell: Some("sh".into()),
        }));
        assert!(run_hook("test_hook", &action).is_ok());
    }

    #[test]
    fn run_hook_list_runs_all_commands() {
        let action = HookAction::List(vec![
            RawCommand::Simple("true".into()),
            RawCommand::Simple("true".into()),
        ]);
        assert!(run_hook("test_hook", &action).is_ok());
    }

    #[test]
    fn run_hook_list_stops_on_first_failure() {
        let action = HookAction::List(vec![
            RawCommand::Simple("false".into()),
            RawCommand::Simple("true".into()),
        ]);
        assert!(run_hook("test_hook", &action).is_err());
    }

    #[test]
    fn spawn_cmd_with_cwd_sets_working_directory() {
        let cmd = EnvyCommand {
            cmd: "pwd".into(),
            cwd: Some("/tmp".into()),
            shell: "sh".into(),
        };
        assert!(spawn_cmd(&cmd, "pwd_test").is_ok());
    }
}
