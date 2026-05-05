use anyhow::{Context, Result, bail};
use std::process::Command;

use crate::config::{DEFAULT_PROFILE, EnvyCommand, EnvyConfig, RawCommand};
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

/// Runs a single hook by name (e.g. "before_up"). Aborts with an error if it exits non-zero.
pub fn run_hook(label: &str, raw: &RawCommand) -> Result<()> {
    let cmd = EnvyCommand::from(raw.clone());
    output::step(&format!("Running hook '{}'", label));
    spawn_cmd(&cmd, label)?;
    output::success(&format!("Hook '{}' succeeded", label));
    Ok(())
}

pub fn run(name: &str) -> Result<()> {
    let mut config = EnvyConfig::load_default()?;

    // `envy <cmd>` has no --profile flag; fall back to ENVY_PROFILE env var.
    let profile = std::env::var("ENVY_PROFILE").unwrap_or_else(|_| DEFAULT_PROFILE.to_string());

    let raw = config
        .commands
        .remove(name)
        .filter(|cmd| cmd.is_active_for(&profile));

    let cmd = match raw {
        Some(raw) => EnvyCommand::from(raw),
        None => {
            let available: Vec<&str> = config
                .commands
                .iter()
                .filter(|(_, cmd)| cmd.is_active_for(&profile))
                .map(|(k, _)| k.as_str())
                .collect();
            if available.is_empty() {
                bail!(
                    "Unknown command '{}'. No commands are defined for profile '{}'.",
                    name,
                    profile
                );
            } else {
                bail!(
                    "Unknown command '{}'. Available (profile: {}): {}",
                    name,
                    profile,
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
    use crate::config::CommandConfig;

    #[test]
    fn run_hook_with_succeeding_command_returns_ok() {
        let raw = RawCommand::Simple("true".into());
        assert!(run_hook("test_hook", &raw).is_ok());
    }

    #[test]
    fn run_hook_with_failing_command_returns_err() {
        let raw = RawCommand::Simple("false".into());
        let err = run_hook("test_hook", &raw).unwrap_err();
        assert!(err.to_string().contains("test_hook"));
    }

    #[test]
    fn run_hook_with_custom_shell_uses_that_shell() {
        let raw = RawCommand::Configured(CommandConfig {
            cmd: "true".into(),
            cwd: None,
            shell: Some("sh".into()),
            profiles: None,
        });
        assert!(run_hook("test_hook", &raw).is_ok());
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
