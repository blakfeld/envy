use crate::config::{DEFAULT_PROFILE, EnvyConfig};

/// Prints each command name from devy.yml that is active for the current profile,
/// one per line. Exits silently if devy.yml does not exist — callers are shell
/// completion functions that must not produce error output.
#[mutants::skip] // returns () and only prints to stdout — not observable in unit tests
pub fn run() {
    let profile = std::env::var("DEVY_PROFILE").unwrap_or_else(|_| DEFAULT_PROFILE.to_string());

    if let Ok(config) = EnvyConfig::load_default() {
        let mut names: Vec<&str> = config
            .commands
            .iter()
            .filter(|(_, cmd)| cmd.is_active_for(&profile))
            .map(|(name, _)| name.as_str())
            .collect();
        names.sort_unstable();
        for name in names {
            println!("{}", name);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{CommandConfig, EnvyConfig, RawCommand};
    use std::collections::HashMap;

    fn config_with_commands(cmds: &[(&str, Option<Vec<&str>>)]) -> EnvyConfig {
        let mut commands = HashMap::new();
        for (name, profiles) in cmds {
            let raw = match profiles {
                None => RawCommand::Simple(format!("echo {}", name)),
                Some(ps) => RawCommand::Configured(CommandConfig {
                    cmd: format!("echo {}", name),
                    cwd: None,
                    shell: None,
                    profiles: Some(ps.iter().map(|s| s.to_string()).collect()),
                }),
            };
            commands.insert(name.to_string(), raw);
        }
        EnvyConfig {
            name: None,
            dependencies: vec![],
            environment: HashMap::new(),
            commands,
            secrets: None,
            hooks: Default::default(),
        }
    }

    #[test]
    fn active_commands_filtered_by_profile() {
        let config = config_with_commands(&[
            ("dev-only", Some(vec!["dev"])),
            ("prod-only", Some(vec!["prod"])),
            ("always", None),
        ]);
        let active: Vec<_> = config
            .commands
            .iter()
            .filter(|(_, cmd)| cmd.is_active_for("dev"))
            .map(|(n, _)| n.as_str())
            .collect();
        assert!(active.contains(&"dev-only"));
        assert!(active.contains(&"always"));
        assert!(!active.contains(&"prod-only"));
    }

    #[test]
    fn all_commands_active_when_no_profiles_set() {
        let config = config_with_commands(&[("build", None), ("test", None)]);
        let active: Vec<_> = config
            .commands
            .iter()
            .filter(|(_, cmd)| cmd.is_active_for("anything"))
            .collect();
        assert_eq!(active.len(), 2);
    }
}
