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
    /// Shell command to run immediately after the dependency is freshly installed.
    /// Runs once per install, not on subsequent `envy up` calls when the dep is
    /// already present. Re-runs if the dep is removed and reinstalled (e.g. after
    /// `envy up --update`). Not recorded in envy.lock, so not idempotent across
    /// installs.
    pub after_install: Option<String>,
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
    pub after_install: Option<String>,
    pub extra: HashMap<String, serde_yaml::Value>,
}

impl Dependency {
    pub fn simple(name: &str) -> Self {
        Dependency {
            name: name.to_string(),
            version: None,
            tap: None,
            profiles: None,
            after_install: None,
            extra: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub fn with_extra(name: &str, extra: HashMap<String, serde_yaml::Value>) -> Self {
        Self { extra, ..Self::simple(name) }
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
        let path = Self::find_config()
            .ok_or_else(|| anyhow::anyhow!("envy.yml not found — are you inside an envy project?"))?;
        Self::load(&path)
    }

    /// Walks from the current directory up to the nearest `.git` root or `$HOME`,
    /// whichever comes first, looking for `envy.yml`.
    ///
    /// Stopping at the git root prevents a malicious or unrelated `envy.yml` planted
    /// in a parent directory from being picked up and having its hooks executed.
    pub(crate) fn find_config() -> Option<std::path::PathBuf> {
        let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let candidate = dir.join("envy.yml");
            if candidate.exists() {
                return Some(candidate);
            }
            // Stop at repository root: a .git here means we've already searched
            // the entire current project without finding envy.yml.
            if dir.join(".git").exists() {
                return None;
            }
            // Never walk above $HOME to limit blast radius in non-git trees.
            if Some(&dir) == home.as_ref() {
                return None;
            }
            match dir.parent() {
                Some(parent) => dir = parent.to_path_buf(),
                None => return None,
            }
        }
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
                    after_install: None,
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
                            after_install: cfg.after_install,
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
            after_install: None,
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
            after_install: None,
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
            after_install: None,
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

    #[test]
    fn normalized_deps_after_install_preserved() {
        let yaml = "dependencies:\n  - mysql:\n      after_install: \"mysql_secure_installation\"\n";
        let config: EnvyConfig = serde_yaml::from_str(yaml).unwrap();
        let deps = config.normalized_dependencies("dev");
        assert_eq!(
            deps[0].after_install.as_deref(),
            Some("mysql_secure_installation")
        );
    }

    #[test]
    fn normalized_deps_after_install_absent_is_none() {
        let yaml = "dependencies:\n  - node\n";
        let config: EnvyConfig = serde_yaml::from_str(yaml).unwrap();
        let deps = config.normalized_dependencies("dev");
        assert!(deps[0].after_install.is_none());
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

    // ── EnvyConfig::find_config ───────────────────────────────────────────────

    // Helper: changes cwd to `dir`, calls find_config, restores cwd.
    // NOTE: std::env::set_current_dir is not thread-safe. Tests using this
    // helper are serialised by ENV_CD_LOCK to avoid interfering with other tests.
    static ENV_CD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_cwd<F: FnOnce() -> Option<std::path::PathBuf>>(dir: &std::path::Path, f: F) -> Option<std::path::PathBuf> {
        let _guard = ENV_CD_LOCK.lock().unwrap();
        let orig = std::env::current_dir().ok();
        std::env::set_current_dir(dir).unwrap();
        let result = f();
        if let Some(o) = orig { let _ = std::env::set_current_dir(o); }
        result
    }

    #[test]
    fn find_config_finds_file_at_git_root_from_subdirectory() {
        // Layout: root/.git, root/envy.yml, root/a/b/ (CWD)
        let root = tmp_dir();
        let sub = root.join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("envy.yml"), "name: test\n").unwrap();

        let found = with_cwd(&sub, EnvyConfig::find_config);
        let expected = root.canonicalize().unwrap().join("envy.yml");
        assert_eq!(found, Some(expected));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_config_stops_at_git_root_when_no_envy_yml() {
        // Layout: root/.git (no envy.yml), root/a/ (CWD)
        let root = tmp_dir();
        let sub = root.join("a");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();

        let found = with_cwd(&sub, EnvyConfig::find_config);
        assert!(found.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_config_finds_envy_yml_in_current_dir() {
        // envy.yml in CWD itself — no walking needed.
        let root = tmp_dir();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("envy.yml"), "name: test\n").unwrap();

        let found = with_cwd(&root, EnvyConfig::find_config);
        let expected = root.canonicalize().unwrap().join("envy.yml");
        assert_eq!(found, Some(expected));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_config_returns_none_in_dir_with_no_git_and_no_envy_yml() {
        // A temp dir with no .git and no envy.yml above it (up to HOME).
        // create a two-level hierarchy so it has a parent to walk to.
        let root = tmp_dir();
        let sub = root.join("inner");
        std::fs::create_dir_all(&sub).unwrap();
        // No envy.yml, no .git → walks up until hitting HOME or root → returns None.
        // (On most systems this terminates when it hits an existing git root higher up
        // or HOME; either way the specific envy.yml in this tree doesn't exist.)
        let found = with_cwd(&sub, EnvyConfig::find_config);
        // We can't assert None here unconditionally since there might be an envy.yml
        // higher in the real tree; just verify it doesn't find one IN our temp root.
        if let Some(ref p) = found {
            assert!(!p.starts_with(&root), "find_config must not return a path inside our tempdir that has no envy.yml");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
