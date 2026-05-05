mod generic;
mod mysql;
mod node;
mod redis;
mod ruby;
mod rust;
mod typescript;

use anyhow::{Context, Result};
use std::process::Command;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

pub trait Module {
    /// Whether this module manages a background service.
    fn is_service(&self) -> bool {
        false
    }

    /// The install source recorded in envy.lock (e.g. "homebrew", "rustup").
    fn source(&self) -> &'static str {
        "homebrew"
    }

    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool>;
    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()>;

    fn is_running(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<bool> {
        Ok(true)
    }

    fn start(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<()> {
        Ok(())
    }

    fn stop(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<()> {
        Ok(())
    }

    /// Probes the service directly to confirm it is accepting connections.
    /// Override in service modules; default always passes.
    fn health_check(&self, _dep: &Dependency) -> Result<()> {
        Ok(())
    }

    /// Polls `health_check` until the service is ready or attempts are exhausted.
    fn wait_for_ready(&self, dep: &Dependency) -> Result<()> {
        const MAX: u32 = 10;
        const SLEEP_MS: u64 = 500;
        for attempt in 1..=MAX {
            match self.health_check(dep) {
                Ok(()) => return Ok(()),
                Err(_) if attempt < MAX => {
                    std::thread::sleep(std::time::Duration::from_millis(SLEEP_MS));
                }
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!("{} did not become healthy after {MAX} attempts", dep.name)
                    });
                }
            }
        }
        Ok(())
    }

    /// Returns the exact version string currently installed.
    /// Default delegates to the package manager; override for non-brew sources.
    fn resolved_version(
        &self,
        pm: &dyn PackageManager,
        dep: &Dependency,
    ) -> Result<Option<String>> {
        pm.resolved_version(dep)
    }
}

/// Resolves a dependency name to its module, falling back to the generic brew module.
pub fn get(name: &str) -> Box<dyn Module> {
    match name {
        "mysql" => Box::new(mysql::MysqlModule),
        "redis" => Box::new(redis::RedisModule),
        "rust" | "rustup" => Box::new(rust::RustModule),
        "python" | "python3" => Box::new(BrewAliasModule("python")),
        "java" | "openjdk" => Box::new(BrewAliasModule("openjdk")),
        "node" | "nodejs" | "javascript" | "js" => Box::new(node::NodeModule),
        "typescript" | "ts" => Box::new(typescript::TypeScriptModule),
        "go" | "golang" => Box::new(BrewAliasModule("go")),
        "ruby" => Box::new(ruby::RubyModule),
        _ => Box::new(generic::GenericModule),
    }
}

/// Maps a dependency name to a different Homebrew formula.
struct BrewAliasModule(&'static str);

impl Module for BrewAliasModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&brew_dep(dep, self.0))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&brew_dep(dep, self.0))
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Runs a command, inheriting stdio, and bails on non-zero exit.
pub(super) fn run_cmd(prog: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(prog)
        .args(args)
        .status()
        .with_context(|| format!("failed to start `{prog}`"))?;
    if !status.success() {
        anyhow::bail!("`{prog} {}` failed", args.join(" "));
    }
    Ok(())
}

/// Reads a YAML sequence of strings from dep.extra.
pub(super) fn extra_strs(dep: &Dependency, key: &str) -> Vec<String> {
    dep.extra
        .get(key)
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Returns a copy of dep with a different brew formula name, preserving version/tap/extra.
pub(super) fn brew_dep(dep: &Dependency, formula: &str) -> Dependency {
    Dependency {
        name: formula.to_string(),
        version: dep.version.clone(),
        tap: dep.tap.clone(),
        profiles: dep.profiles.clone(),
        extra: dep.extra.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::collections::HashMap;

    #[allow(dead_code)]
    struct MockPm;
    impl crate::package_manager::PackageManager for MockPm {
        fn name(&self) -> &str {
            "mock"
        }
        fn is_available(&self) -> bool {
            true
        }
        fn bootstrap(&self) -> Result<()> {
            Ok(())
        }
        fn is_package_installed(&self, _: &Dependency) -> Result<bool> {
            Ok(false)
        }
        fn install_package(&self, _: &Dependency) -> Result<()> {
            Ok(())
        }
        fn is_service_running(&self, _: &str) -> Result<bool> {
            Ok(false)
        }
        fn start_service(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn stop_service(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn resolved_version(&self, _: &Dependency) -> Result<Option<String>> {
            Ok(None)
        }
    }

    // ── get ───────────────────────────────────────────────────────────────────

    #[test]
    fn get_mysql_is_service() {
        assert!(get("mysql").is_service());
    }

    #[test]
    fn get_redis_is_service() {
        assert!(get("redis").is_service());
    }

    #[test]
    fn get_rust_source_is_rustup() {
        assert_eq!(get("rust").source(), "rustup");
        assert_eq!(get("rustup").source(), "rustup");
    }

    #[test]
    fn get_node_aliases_resolve() {
        for name in &["node", "nodejs", "javascript", "js"] {
            let m = get(name);
            assert!(!m.is_service(), "{} should not be a service", name);
        }
    }

    #[test]
    fn get_language_aliases_are_not_services() {
        for name in &[
            "python",
            "python3",
            "java",
            "openjdk",
            "go",
            "golang",
            "ruby",
            "typescript",
            "ts",
        ] {
            assert!(!get(name).is_service(), "{} should not be a service", name);
        }
    }

    #[test]
    fn get_unknown_falls_back_to_generic() {
        let m = get("somerandompkg");
        assert!(!m.is_service());
        assert_eq!(m.source(), "homebrew");
    }

    // ── extra_strs ────────────────────────────────────────────────────────────

    #[test]
    fn extra_strs_missing_key_returns_empty() {
        let dep = Dependency::simple("node");
        assert!(extra_strs(&dep, "global_packages").is_empty());
    }

    #[test]
    fn extra_strs_sequence_returns_strings() {
        let mut extra = HashMap::new();
        extra.insert(
            "global_packages".to_string(),
            serde_yaml::Value::Sequence(vec![
                serde_yaml::Value::String("typescript".into()),
                serde_yaml::Value::String("eslint".into()),
            ]),
        );
        let dep = Dependency {
            name: "node".into(),
            version: None,
            tap: None,
            profiles: None,
            extra,
        };
        let pkgs = extra_strs(&dep, "global_packages");
        assert_eq!(pkgs, vec!["typescript", "eslint"]);
    }

    #[test]
    fn extra_strs_non_sequence_value_returns_empty() {
        let mut extra = HashMap::new();
        extra.insert(
            "global_packages".to_string(),
            serde_yaml::Value::String("ts".into()),
        );
        let dep = Dependency {
            name: "node".into(),
            version: None,
            tap: None,
            profiles: None,
            extra,
        };
        assert!(extra_strs(&dep, "global_packages").is_empty());
    }

    // ── brew_dep ──────────────────────────────────────────────────────────────

    #[test]
    fn brew_dep_replaces_name_preserves_other_fields() {
        let dep = Dependency {
            name: "ruby".into(),
            version: Some("3.2".into()),
            tap: Some("homebrew/core".into()),
            profiles: Some(vec!["dev".into()]),
            extra: HashMap::new(),
        };
        let remapped = brew_dep(&dep, "ruby@3.2");
        assert_eq!(remapped.name, "ruby@3.2");
        assert_eq!(remapped.version, Some("3.2".into()));
        assert_eq!(remapped.tap, Some("homebrew/core".into()));
        assert_eq!(remapped.profiles, Some(vec!["dev".into()]));
    }

    // ── wait_for_ready ────────────────────────────────────────────────────────

    struct HealthyModule;
    impl Module for HealthyModule {
        fn is_installed(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<bool> {
            Ok(true)
        }
        fn install(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<()> {
            Ok(())
        }
        fn health_check(&self, _: &Dependency) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn wait_for_ready_succeeds_immediately_when_healthy() {
        let dep = Dependency::simple("testservice");
        HealthyModule.wait_for_ready(&dep).unwrap();
    }

    struct SickModule;
    impl Module for SickModule {
        fn is_installed(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<bool> {
            Ok(true)
        }
        fn install(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<()> {
            Ok(())
        }
        fn health_check(&self, _: &Dependency) -> Result<()> {
            anyhow::bail!("not healthy")
        }
    }

    #[test]
    #[ignore = "intentionally slow: polls 10 times with 500ms sleep"]
    fn wait_for_ready_fails_with_context_after_max_attempts() {
        let dep = Dependency::simple("testservice");
        let err = SickModule.wait_for_ready(&dep).unwrap_err();
        assert!(err.to_string().contains("testservice"));
        assert!(err.to_string().contains("10 attempts"));
    }
}
