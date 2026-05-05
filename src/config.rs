use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

pub const DEFAULT_PROFILE: &str = "dev";

// ── Commands ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RawCommand {
    Simple(String),
    Configured(CommandConfig),
}

impl RawCommand {
    pub fn is_active_for(&self, profile: &str) -> bool {
        match self {
            RawCommand::Simple(_) => true,
            RawCommand::Configured(c) => match &c.profiles {
                None => true,
                Some(ps) => ps.iter().any(|p| p == profile),
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommandConfig {
    pub cmd: String,
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub profiles: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct EnvyCommand {
    pub cmd: String,
    pub cwd: Option<String>,
    pub shell: String,
}

impl From<RawCommand> for EnvyCommand {
    fn from(raw: RawCommand) -> Self {
        match raw {
            RawCommand::Simple(cmd) => EnvyCommand {
                cmd,
                cwd: None,
                shell: "sh".into(),
            },
            RawCommand::Configured(c) => EnvyCommand {
                cmd: c.cmd,
                cwd: c.cwd,
                shell: c.shell.unwrap_or_else(|| "sh".into()),
            },
        }
    }
}

// ── Hooks ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct HooksConfig {
    pub before_up: Option<RawCommand>,
    pub after_up: Option<RawCommand>,
    pub before_down: Option<RawCommand>,
    pub after_down: Option<RawCommand>,
}

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EnvyConfig {
    pub name: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<RawDependency>,
    #[serde(default)]
    pub environment: HashMap<String, String>,
    #[serde(default)]
    pub commands: HashMap<String, RawCommand>,
    /// Path to an ejson file whose decrypted values are merged into the environment.
    pub secrets: Option<String>,
    #[serde(default)]
    pub hooks: HooksConfig,
}

/// Supports two forms in YAML:
///   - python
///   - mysql:
///     - version: "8.1"
///     - port: 3307
///     - profiles: [dev, test]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawDependency {
    Simple(String),
    Configured(HashMap<String, Option<DepConfig>>),
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct DepConfig {
    pub version: Option<String>,
    pub tap: Option<String>,
    pub profiles: Option<Vec<String>>,
    /// Module-specific keys (e.g. port, cli_args) are captured here.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

/// Normalized, flat representation used throughout the rest of the codebase.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version: Option<String>,
    pub tap: Option<String>,
    pub profiles: Option<Vec<String>>,
    pub extra: HashMap<String, serde_yaml::Value>,
}

impl Dependency {
    pub fn simple(name: &str) -> Self {
        Dependency {
            name: name.to_string(),
            version: None,
            tap: None,
            profiles: None,
            extra: HashMap::new(),
        }
    }

    pub fn versioned_name(&self) -> String {
        match &self.version {
            Some(v) => format!("{}@{}", self.name, v),
            None => self.name.clone(),
        }
    }

    /// Returns true if this dep is active for the given profile.
    /// Deps with no `profiles` key are always active.
    pub fn is_active_for(&self, profile: &str) -> bool {
        match &self.profiles {
            None => true,
            Some(ps) => ps.iter().any(|p| p == profile),
        }
    }
}

impl EnvyConfig {
    pub fn load_default() -> Result<Self> {
        Self::load(Path::new("envy.yml"))
            .context("Failed to load envy.yml — are you in the right directory?")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))
    }

    /// Returns dependencies active for the given profile.
    /// Entries with no `profiles` key are always included.
    pub fn normalized_dependencies(&self, profile: &str) -> Vec<Dependency> {
        self.dependencies
            .iter()
            .flat_map(|raw| match raw {
                RawDependency::Simple(name) => vec![Dependency {
                    name: name.clone(),
                    version: None,
                    tap: None,
                    profiles: None,
                    extra: HashMap::new(),
                }],
                RawDependency::Configured(map) => map
                    .iter()
                    .map(|(name, cfg)| {
                        let cfg = cfg.clone().unwrap_or_default();
                        Dependency {
                            name: name.clone(),
                            version: cfg.version,
                            tap: cfg.tap,
                            profiles: cfg.profiles,
                            extra: cfg.extra,
                        }
                    })
                    .collect(),
            })
            .filter(|dep| dep.is_active_for(profile))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "envy_cfg_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ── RawCommand::is_active_for ─────────────────────────────────────────────

    #[test]
    fn simple_command_always_active() {
        let cmd = RawCommand::Simple("echo hi".into());
        assert!(cmd.is_active_for("dev"));
        assert!(cmd.is_active_for("prod"));
    }

    #[test]
    fn configured_command_no_profiles_always_active() {
        let cmd = RawCommand::Configured(CommandConfig {
            cmd: "echo hi".into(),
            cwd: None,
            shell: None,
            profiles: None,
        });
        assert!(cmd.is_active_for("dev"));
    }

    #[test]
    fn configured_command_matching_profile() {
        let cmd = RawCommand::Configured(CommandConfig {
            cmd: "echo hi".into(),
            cwd: None,
            shell: None,
            profiles: Some(vec!["dev".into(), "test".into()]),
        });
        assert!(cmd.is_active_for("dev"));
        assert!(cmd.is_active_for("test"));
    }

    #[test]
    fn configured_command_non_matching_profile() {
        let cmd = RawCommand::Configured(CommandConfig {
            cmd: "echo hi".into(),
            cwd: None,
            shell: None,
            profiles: Some(vec!["prod".into()]),
        });
        assert!(!cmd.is_active_for("dev"));
    }

    // ── EnvyCommand::from ─────────────────────────────────────────────────────

    #[test]
    fn from_simple_defaults_to_sh() {
        let raw = RawCommand::Simple("echo hi".into());
        let cmd = EnvyCommand::from(raw);
        assert_eq!(cmd.cmd, "echo hi");
        assert_eq!(cmd.shell, "sh");
        assert!(cmd.cwd.is_none());
    }

    #[test]
    fn from_configured_uses_custom_shell_and_cwd() {
        let raw = RawCommand::Configured(CommandConfig {
            cmd: "make build".into(),
            cwd: Some("/tmp".into()),
            shell: Some("bash".into()),
            profiles: None,
        });
        let cmd = EnvyCommand::from(raw);
        assert_eq!(cmd.cmd, "make build");
        assert_eq!(cmd.shell, "bash");
        assert_eq!(cmd.cwd, Some("/tmp".into()));
    }

    #[test]
    fn from_configured_shell_none_defaults_to_sh() {
        let raw = RawCommand::Configured(CommandConfig {
            cmd: "echo".into(),
            cwd: None,
            shell: None,
            profiles: None,
        });
        let cmd = EnvyCommand::from(raw);
        assert_eq!(cmd.shell, "sh");
        assert!(cmd.cwd.is_none());
    }

    // ── Dependency::versioned_name ────────────────────────────────────────────

    #[test]
    fn versioned_name_no_version() {
        let dep = Dependency::simple("node");
        assert_eq!(dep.versioned_name(), "node");
    }

    #[test]
    fn versioned_name_with_version() {
        let dep = Dependency {
            name: "node".into(),
            version: Some("20".into()),
            tap: None,
            profiles: None,
            extra: HashMap::new(),
        };
        assert_eq!(dep.versioned_name(), "node@20");
    }

    // ── Dependency::is_active_for ─────────────────────────────────────────────

    #[test]
    fn dep_no_profiles_always_active() {
        let dep = Dependency::simple("node");
        assert!(dep.is_active_for("dev"));
        assert!(dep.is_active_for("prod"));
    }

    #[test]
    fn dep_matching_profile_active() {
        let dep = Dependency {
            name: "node".into(),
            version: None,
            tap: None,
            profiles: Some(vec!["dev".into()]),
            extra: HashMap::new(),
        };
        assert!(dep.is_active_for("dev"));
    }

    #[test]
    fn dep_non_matching_profile_inactive() {
        let dep = Dependency {
            name: "node".into(),
            version: None,
            tap: None,
            profiles: Some(vec!["prod".into()]),
            extra: HashMap::new(),
        };
        assert!(!dep.is_active_for("dev"));
    }

    // ── EnvyConfig::normalized_dependencies ──────────────────────────────────

    #[test]
    fn normalized_deps_simple_included_for_any_profile() {
        let yaml = "dependencies:\n  - node\n  - python\n";
        let config: EnvyConfig = serde_yaml::from_str(yaml).unwrap();
        let deps = config.normalized_dependencies("dev");
        let names: Vec<_> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"node"));
        assert!(names.contains(&"python"));
    }

    #[test]
    fn normalized_deps_profile_filtered_out() {
        let yaml = "dependencies:\n  - node:\n      profiles: [prod]\n  - python\n";
        let config: EnvyConfig = serde_yaml::from_str(yaml).unwrap();
        let deps = config.normalized_dependencies("dev");
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "python");
    }

    #[test]
    fn normalized_deps_profile_included_when_matching() {
        let yaml = "dependencies:\n  - node:\n      profiles: [dev]\n";
        let config: EnvyConfig = serde_yaml::from_str(yaml).unwrap();
        let deps = config.normalized_dependencies("dev");
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "node");
    }

    #[test]
    fn normalized_deps_version_preserved() {
        let yaml = "dependencies:\n  - node:\n      version: \"20\"\n";
        let config: EnvyConfig = serde_yaml::from_str(yaml).unwrap();
        let deps = config.normalized_dependencies("dev");
        assert_eq!(deps[0].version, Some("20".into()));
    }

    // ── EnvyConfig::load ──────────────────────────────────────────────────────

    #[test]
    fn load_valid_yaml_ok() {
        let dir = tmp_dir();
        let path = dir.join("envy.yml");
        std::fs::write(&path, "name: test\n").unwrap();
        assert!(EnvyConfig::load(&path).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_err() {
        let path = std::path::Path::new("/nonexistent/envy_test_missing.yml");
        assert!(EnvyConfig::load(path).is_err());
    }

    #[test]
    fn load_invalid_yaml_err() {
        let dir = tmp_dir();
        let path = dir.join("envy.yml");
        std::fs::write(&path, "dependencies: [unclosed\n").unwrap();
        assert!(EnvyConfig::load(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
