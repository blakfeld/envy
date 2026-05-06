use anyhow::{Result, bail};

pub fn run(shell: &str) -> Result<()> {
    let snippet = match shell {
        "zsh" => ZSH_SNIPPET,
        "bash" => BASH_SNIPPET,
        "fish" => FISH_SNIPPET,
        other => bail!(
            "Unsupported shell '{}'. Supported shells: zsh, bash, fish",
            other
        ),
    };
    print!("{}", snippet);
    Ok(())
}

// Each snippet defines:
//   1. An `envy` shell function that intercepts `envy up` to activate shadowenv
//      in the current shell session after installation.
//   2. A completion function/block that provides tab-completion for all built-in
//      subcommands and dynamically completes user-defined commands from devy.yml
//      by calling `command devy _commands`.
//
// MAINTENANCE: All three snippets (ZSH, BASH, FISH) must be kept in sync.
// When adding a new subcommand, update all three constants AND the test lists in
// `all_builtin_subcommands_appear_in_*_snippet` below.

const ZSH_SNIPPET: &str = r#"
devy() {
  if [ "$1" = "up" ]; then
    command devy "$@" && eval "$(shadowenv hook zsh)"
  elif [ "$1" = "hook" ]; then
    command devy hook zsh
  else
    command devy "$@"
  fi
}

_devy() {
  local -a subcmds
  subcmds=(
    'up:Set up the development environment'
    'down:Stop all services'
    'services:List services and their status'
    'start:Start a named service'
    'stop:Stop a named service'
    'restart:Restart a named service'
    'status:Show install and environment status'
    'check:Validate the environment without making changes'
    'init:Create an empty devy.yml'
    'hook:Print shell integration snippet'
  )
  local user_cmd
  while IFS= read -r user_cmd; do
    [[ -n "$user_cmd" ]] && subcmds+=("$user_cmd")
  done < <(command devy _commands 2>/dev/null)

  if (( CURRENT == 2 )); then
    _describe 'command' subcmds
    return
  fi

  case "${words[2]}" in
    up)
      _arguments \
        '--update[Re-resolve all versions and rewrite devy.lock]' \
        '--dry-run[Check status without making changes]' \
        '--profile[Profile to activate]:profile'
      ;;
    down|status|check|services)
      _arguments '--profile[Profile to use]:profile'
      ;;
    start|stop|restart)
      _arguments \
        '1:service name' \
        '--profile[Profile to use]:profile'
      ;;
    init)
      _arguments '--force[Overwrite an existing devy.yml]'
      ;;
    hook)
      _values 'shell' zsh bash fish
      ;;
  esac
}

compdef _devy devy
"#;

const BASH_SNIPPET: &str = r#"
devy() {
  if [ "$1" = "up" ]; then
    command devy "$@" && eval "$(shadowenv hook bash)"
  elif [ "$1" = "hook" ]; then
    command devy hook bash
  else
    command devy "$@"
  fi
}

_devy_completions() {
  local cur="${COMP_WORDS[COMP_CWORD]}"
  local subcmds="up down services start stop restart status check init hook"
  local user_cmds
  user_cmds=$(command devy _commands 2>/dev/null)
  [ -n "$user_cmds" ] && subcmds="$subcmds $user_cmds"

  if [ "$COMP_CWORD" -eq 1 ]; then
    COMPREPLY=($(compgen -W "$subcmds" -- "$cur"))
    return
  fi

  case "${COMP_WORDS[1]}" in
    up)
      COMPREPLY=($(compgen -W "--update --dry-run --profile" -- "$cur"))
      ;;
    down|status|check|services)
      COMPREPLY=($(compgen -W "--profile" -- "$cur"))
      ;;
    start|stop|restart)
      COMPREPLY=($(compgen -W "--profile" -- "$cur"))
      ;;
    init)
      COMPREPLY=($(compgen -W "--force" -- "$cur"))
      ;;
    hook)
      COMPREPLY=($(compgen -W "zsh bash fish" -- "$cur"))
      ;;
  esac
}

complete -F _devy_completions devy
"#;

const FISH_SNIPPET: &str = r#"
function devy
  if test "$argv[1]" = "up"
    command devy $argv; and shadowenv hook fish | source
  else if test "$argv[1]" = "hook"
    command devy hook fish
  else
    command devy $argv
  end
end

function __devy_user_commands
  command devy _commands 2>/dev/null
end

function __devy_no_subcommand
  not __fish_seen_subcommand_from up down services start stop restart status check init hook
end

complete -c devy -f
complete -c devy -n __devy_no_subcommand -a up       -d "Set up the development environment"
complete -c devy -n __devy_no_subcommand -a down     -d "Stop all services"
complete -c devy -n __devy_no_subcommand -a services -d "List services and their status"
complete -c devy -n __devy_no_subcommand -a start    -d "Start a named service"
complete -c devy -n __devy_no_subcommand -a stop     -d "Stop a named service"
complete -c devy -n __devy_no_subcommand -a restart  -d "Restart a named service"
complete -c devy -n __devy_no_subcommand -a status   -d "Show install and environment status"
complete -c devy -n __devy_no_subcommand -a check    -d "Validate the environment"
complete -c devy -n __devy_no_subcommand -a init     -d "Scaffold a devy.yml"
complete -c devy -n __devy_no_subcommand -a hook     -d "Print shell integration snippet"
complete -c devy -n __devy_no_subcommand -a "(__devy_user_commands)" -d "User-defined command"
complete -c devy -n "__fish_seen_subcommand_from hook" -a "zsh bash fish"
complete -c devy -n "__fish_seen_subcommand_from up" -l update  -d "Re-resolve all versions"
complete -c devy -n "__fish_seen_subcommand_from up" -l dry-run -d "Check without making changes"
complete -c devy -n "__fish_seen_subcommand_from up down services start stop restart status check" -l profile -d "Profile to use"
complete -c devy -n "__fish_seen_subcommand_from init" -l force -d "Overwrite existing devy.yml"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_zsh_succeeds() {
        assert!(run("zsh").is_ok());
    }

    #[test]
    fn run_bash_succeeds() {
        assert!(run("bash").is_ok());
    }

    #[test]
    fn run_fish_succeeds() {
        assert!(run("fish").is_ok());
    }

    #[test]
    fn run_unsupported_shell_returns_error() {
        let err = run("powershell").unwrap_err();
        assert!(err.to_string().contains("Unsupported shell"));
        assert!(err.to_string().contains("powershell"));
    }

    #[test]
    fn zsh_snippet_contains_shadowenv_hook() {
        assert!(ZSH_SNIPPET.contains("shadowenv hook zsh"));
    }

    #[test]
    fn bash_snippet_contains_shadowenv_hook() {
        assert!(BASH_SNIPPET.contains("shadowenv hook bash"));
    }

    #[test]
    fn fish_snippet_contains_shadowenv_hook() {
        assert!(FISH_SNIPPET.contains("shadowenv hook fish"));
    }

    #[test]
    fn zsh_snippet_contains_completion_function() {
        assert!(ZSH_SNIPPET.contains("_devy()"));
        assert!(ZSH_SNIPPET.contains("compdef _devy devy"));
        assert!(ZSH_SNIPPET.contains("_commands"));
    }

    #[test]
    fn bash_snippet_contains_completion_function() {
        assert!(BASH_SNIPPET.contains("_devy_completions"));
        assert!(BASH_SNIPPET.contains("complete -F _devy_completions devy"));
        assert!(BASH_SNIPPET.contains("_commands"));
    }

    #[test]
    fn fish_snippet_contains_completion_directives() {
        assert!(FISH_SNIPPET.contains("complete -c devy"));
        assert!(FISH_SNIPPET.contains("__devy_user_commands"));
        assert!(FISH_SNIPPET.contains("_commands"));
    }

    #[test]
    fn all_builtin_subcommands_appear_in_zsh_snippet() {
        for cmd in &[
            "up", "down", "services", "start", "stop", "restart", "status", "check", "init", "hook",
        ] {
            assert!(ZSH_SNIPPET.contains(cmd), "zsh snippet missing '{}'", cmd);
        }
    }

    #[test]
    fn all_builtin_subcommands_appear_in_bash_snippet() {
        for cmd in &[
            "up", "down", "services", "start", "stop", "restart", "status", "check", "init", "hook",
        ] {
            assert!(BASH_SNIPPET.contains(cmd), "bash snippet missing '{}'", cmd);
        }
    }

    #[test]
    fn all_builtin_subcommands_appear_in_fish_snippet() {
        for cmd in &[
            "up", "down", "services", "start", "stop", "restart", "status", "check", "init", "hook",
        ] {
            assert!(FISH_SNIPPET.contains(cmd), "fish snippet missing '{}'", cmd);
        }
    }
}
