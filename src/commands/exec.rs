use anyhow::{Context, Result, bail};
use std::process::Command;

use crate::config::{DevyCommand, DevyConfig, HookAction};
use crate::output;

const ALLOWED_SHELLS: &[&str] = &["sh", "bash", "zsh", "fish", "cmd", "powershell"];

/// Returns the flag used to pass a command string to the given shell.
/// `cmd.exe` uses `/c`; all POSIX shells and PowerShell accept `-c`.
fn shell_flag(shell: &str) -> &'static str {
    match shell {
        "cmd" => "/c",
        _ => "-c",
    }
}

fn validate_shell(shell: &str) -> Result<()> {
    if shell.contains('/') || shell.contains('\\') {
        bail!(
            "shell '{}' must be a bare name, not a path; permitted shells: {}",
            shell,
            ALLOWED_SHELLS.join(", ")
        );
    }
    if !ALLOWED_SHELLS.contains(&shell) {
        bail!(
            "shell '{}' is not in the allowed list; permitted shells: {}",
            shell,
            ALLOWED_SHELLS.join(", ")
        );
    }
    Ok(())
}

pub(crate) fn spawn_cmd(cmd: &DevyCommand, label: &str) -> Result<()> {
    validate_shell(&cmd.shell).with_context(|| format!("'{}': invalid shell", label))?;
    let mut proc = Command::new(&cmd.shell);
    proc.arg(shell_flag(&cmd.shell)).arg(&cmd.cmd);
    if let Some(cwd) = &cmd.cwd {
        proc.current_dir(cwd);
    }
    let status = proc
        .status()
        .with_context(|| format!("Failed to spawn '{}' via {}", label, cmd.shell))?;
    if !status.success() {
        bail!(
            "'{}' command {:?} exited with status {}",
            label,
            cmd.cmd,
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

/// Runs all commands in a hook. Aborts with an error if any command exits non-zero.
pub fn run_hook(label: &str, action: &HookAction) -> Result<()> {
    let cmds = action.commands();
    let total = cmds.len();
    for (i, raw) in cmds.iter().enumerate() {
        let cmd = DevyCommand::from(raw.clone());
        let tag = if total == 1 {
            format!("'{label}'")
        } else {
            format!("'{label}' ({}/{})", i + 1, total)
        };
        output::step(&format!("Running hook {tag}"));
        spawn_cmd(&cmd, &tag)?;
        output::success(&format!("Hook {tag} succeeded"));
    }
    Ok(())
}

/// Wraps `s` in POSIX single quotes, escaping embedded single quotes.
/// Safe for sh, bash, zsh, and fish. Not used for cmd/powershell.
pub(crate) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn append_extra_args(cmd: DevyCommand, extra_args: &[String]) -> DevyCommand {
    if extra_args.is_empty() {
        return cmd;
    }
    if matches!(cmd.shell.as_str(), "cmd" | "powershell") {
        output::warn(&format!(
            "extra args are not supported with shell '{}' — args ignored",
            cmd.shell
        ));
        return cmd;
    }
    let quoted: Vec<String> = extra_args.iter().map(|a| sh_quote(a)).collect();
    DevyCommand {
        cmd: format!("{} {}", cmd.cmd, quoted.join(" ")),
        ..cmd
    }
}

#[cfg_attr(test, mutants::skip)] // thin I/O wrapper — requires a real devy.yml on disk
pub fn run(name: &str, extra_args: &[String]) -> Result<()> {
    let config = DevyConfig::load_default()?;

    let raw = config.commands.get(name).cloned();

    let cmd = match raw {
        Some(raw) => DevyCommand::from(raw),
        None => {
            let mut available: Vec<&str> = config.commands.keys().map(|k| k.as_str()).collect();
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

    let cmd = append_extra_args(cmd, extra_args);
    output::step(&format!("Running '{}'", name));
    spawn_cmd(&cmd, name)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HookAction, RawCommand};

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
        // Note: error no longer says "hook" — just the label name
        assert!(!err.to_string().starts_with("hook"));
    }

    #[test]
    fn run_hook_with_custom_shell_uses_that_shell() {
        let action = HookAction::Single(RawCommand::Configured {
            cmd: "true".into(),
            cwd: None,
            shell: Some("sh".into()),
        });
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
        let cmd = DevyCommand {
            cmd: "pwd".into(),
            cwd: Some("/tmp".into()),
            shell: "sh".into(),
        };
        assert!(spawn_cmd(&cmd, "pwd_test").is_ok());
    }

    // ── shell_flag ────────────────────────────────────────────────────────────

    #[test]
    fn shell_flag_cmd_uses_slash_c() {
        assert_eq!(shell_flag("cmd"), "/c");
    }

    #[test]
    fn shell_flag_posix_shells_use_dash_c() {
        for shell in &["sh", "bash", "zsh", "fish", "powershell"] {
            assert_eq!(shell_flag(shell), "-c", "{shell} should use -c");
        }
    }

    // ── validate_shell ────────────────────────────────────────────────────────

    #[test]
    fn validate_shell_accepts_allowed_shells() {
        for shell in &["sh", "bash", "zsh", "fish", "cmd", "powershell"] {
            assert!(validate_shell(shell).is_ok(), "should allow '{shell}'");
        }
    }

    #[test]
    fn validate_shell_rejects_path_to_allowed_shell() {
        // Paths are rejected even when the basename is an allowed shell name,
        // because /tmp/sh is not the same as sh.
        assert!(validate_shell("/bin/sh").is_err());
        assert!(validate_shell("/usr/bin/bash").is_err());
        assert!(validate_shell("/usr/local/bin/zsh").is_err());
    }

    #[test]
    fn validate_shell_rejects_unknown_binary() {
        let err = validate_shell("evil-binary").unwrap_err();
        assert!(err.to_string().contains("evil-binary"));
        assert!(err.to_string().contains("not in the allowed list"));
    }

    #[test]
    fn validate_shell_rejects_absolute_path_to_unknown_binary() {
        let err = validate_shell("/tmp/evil").unwrap_err();
        assert!(err.to_string().contains("not a path"));
    }

    // ── append_extra_args ─────────────────────────────────────────────────────

    fn base_cmd(s: &str) -> DevyCommand {
        DevyCommand {
            cmd: s.into(),
            cwd: None,
            shell: "sh".into(),
        }
    }

    #[test]
    fn append_extra_args_empty_leaves_cmd_unchanged() {
        let cmd = append_extra_args(base_cmd("cargo build"), &[]);
        assert_eq!(cmd.cmd, "cargo build");
    }

    #[test]
    fn append_extra_args_appends_space_separated() {
        let extra = vec!["--release".into(), "--target".into(), "x86_64".into()];
        let cmd = append_extra_args(base_cmd("cargo build"), &extra);
        assert_eq!(cmd.cmd, "cargo build '--release' '--target' 'x86_64'");
    }

    #[test]
    fn append_extra_args_quotes_arg_with_space() {
        let extra = vec!["my arg".into()];
        let cmd = append_extra_args(base_cmd("pytest"), &extra);
        assert_eq!(cmd.cmd, "pytest 'my arg'");
    }

    #[test]
    fn append_extra_args_escapes_single_quote() {
        let extra = vec!["it's".into()];
        let cmd = append_extra_args(base_cmd("echo"), &extra);
        assert_eq!(cmd.cmd, "echo 'it'\\''s'");
    }

    #[test]
    fn append_extra_args_preserves_shell_and_cwd() {
        let input = DevyCommand {
            cmd: "make".into(),
            cwd: Some("/tmp".into()),
            shell: "bash".into(),
        };
        let cmd = append_extra_args(input, &["all".into()]);
        assert_eq!(cmd.shell, "bash");
        assert_eq!(cmd.cwd, Some("/tmp".into()));
    }

    #[test]
    fn append_extra_args_cmd_shell_returns_unchanged() {
        let input = DevyCommand {
            cmd: "dir".into(),
            cwd: None,
            shell: "cmd".into(),
        };
        let cmd = append_extra_args(input, &["/w".into()]);
        assert_eq!(cmd.cmd, "dir", "cmd shell must ignore extra args");
    }

    #[test]
    fn append_extra_args_powershell_returns_unchanged() {
        let input = DevyCommand {
            cmd: "Get-ChildItem".into(),
            cwd: None,
            shell: "powershell".into(),
        };
        let cmd = append_extra_args(input, &["-Path".into(), "C:\\".into()]);
        assert_eq!(
            cmd.cmd, "Get-ChildItem",
            "powershell shell must ignore extra args"
        );
    }

    #[test]
    fn spawn_cmd_rejects_disallowed_shell() {
        let cmd = DevyCommand {
            cmd: "true".into(),
            cwd: None,
            shell: "not-a-shell".into(),
        };
        assert!(spawn_cmd(&cmd, "test").is_err());
    }
}
