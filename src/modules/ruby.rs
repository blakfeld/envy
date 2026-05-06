use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use which::which;

use crate::config::Dependency;
use crate::output;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep, run_cmd};

pub struct RubyModule;

fn rbenv_root() -> Option<String> {
    if let Ok(root) = std::env::var("RBENV_ROOT")
        && !root.is_empty()
    {
        return Some(root);
    }
    std::env::var("HOME").ok().map(|h| format!("{h}/.rbenv"))
}

fn rbenv_version_installed(version: &str) -> Result<bool> {
    let out = Command::new("rbenv")
        .args(["versions", "--bare"])
        .output()
        .context("Failed to run `rbenv versions --bare`")?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout.lines().any(|l| l.trim() == version))
}

fn rbenv_global() -> Option<String> {
    let out = Command::new("rbenv").arg("global").output().ok()?;
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if s.is_empty() || s == "system" {
        None
    } else {
        Some(s)
    }
}

impl Module for RubyModule {
    fn source(&self) -> &'static str {
        "rbenv"
    }

    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        if pm.name() == "winget" {
            return pm.is_package_installed(&pm_dep(dep, "RubyInstallerTeam.Ruby.3"));
        }
        if which("rbenv").is_err() {
            return Ok(false);
        }
        match &dep.version {
            Some(v) => rbenv_version_installed(v),
            None => Ok(rbenv_global().is_some()),
        }
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        if pm.name() == "winget" {
            return pm.install_package(&pm_dep(dep, "RubyInstallerTeam.Ruby.3"));
        }

        if which("rbenv").is_err() {
            match pm.name() {
                "apt" => {
                    pm.install_package(&Dependency::simple("rbenv"))?;
                    pm.install_package(&Dependency::simple("ruby-build"))?;
                }
                _ => {
                    pm.install_package(&Dependency::simple("rbenv"))?;
                }
            }
        }

        // Default to a recent stable version if none specified.
        let version = dep.version.as_deref().unwrap_or("3.3.6");
        run_cmd("rbenv", &["install", "--skip-existing", version])?;
        run_cmd("rbenv", &["global", version])?;
        Ok(())
    }

    fn env_vars(&self, _dep: &Dependency) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        if let Some(root) = rbenv_root() {
            vars.insert("RBENV_ROOT".into(), root);
        }
        vars
    }

    fn path_prepends(&self, _dep: &Dependency) -> Vec<String> {
        rbenv_root()
            .map(|root| vec![format!("{root}/bin"), format!("{root}/shims")])
            .unwrap_or_default()
    }

    fn post_setup(&self, _dep: &Dependency, project_root: &Path) -> Result<()> {
        if !project_root.join("Gemfile").exists() {
            return Ok(());
        }
        // Use the shims path directly to avoid PATH ordering issues during `devy up`:
        // shadowenv hasn't been activated yet so rbenv shims may not be on $PATH.
        let bundle = rbenv_root()
            .map(|root| format!("{root}/shims/bundle"))
            .filter(|p| Path::new(p).exists())
            .unwrap_or_else(|| "bundle".into());

        output::step("Running bundle install");
        let status = Command::new(&bundle)
            .arg("install")
            .current_dir(project_root)
            .status()
            .context("Failed to run `bundle install`")?;
        if !status.success() {
            anyhow::bail!("`bundle install` failed");
        }
        output::success("bundle install complete");
        Ok(())
    }

    fn resolved_version(
        &self,
        pm: &dyn PackageManager,
        dep: &Dependency,
    ) -> Result<Option<String>> {
        if pm.name() == "winget" {
            return pm.resolved_version(dep);
        }
        Ok(rbenv_global())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;

    #[test]
    fn ruby_module_is_not_a_service() {
        assert!(!RubyModule.is_service());
    }

    #[test]
    fn ruby_source_is_rbenv() {
        assert_eq!(RubyModule.source(), "rbenv");
    }

    #[test]
    fn ruby_winget_is_installed_delegates_to_pm() {
        let pm = MockPackageManager {
            name: "winget",
            installed: true,
            ..Default::default()
        };
        assert!(
            RubyModule
                .is_installed(&pm, &Dependency::simple("ruby"))
                .unwrap()
        );
    }

    #[test]
    fn ruby_winget_not_installed_when_pm_reports_false() {
        let pm = MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert!(
            !RubyModule
                .is_installed(&pm, &Dependency::simple("ruby"))
                .unwrap()
        );
    }

    #[test]
    fn ruby_winget_install_delegates_to_pm() {
        let pm = MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert!(RubyModule.install(&pm, &Dependency::simple("ruby")).is_ok());
        assert!(!pm.installed_packages.borrow().is_empty());
    }

    #[test]
    fn ruby_winget_install_propagates_pm_error() {
        let pm = MockPackageManager {
            name: "winget",
            install_fails: true,
            ..Default::default()
        };
        assert!(
            RubyModule
                .install(&pm, &Dependency::simple("ruby"))
                .is_err()
        );
    }

    #[test]
    fn ruby_env_vars_contains_rbenv_root_when_home_set() {
        if std::env::var("HOME").is_err() {
            return;
        }
        let dep = Dependency::simple("ruby");
        let vars = RubyModule.env_vars(&dep);
        assert!(vars.contains_key("RBENV_ROOT"), "RBENV_ROOT must be set");
        assert!(!vars["RBENV_ROOT"].is_empty());
    }

    #[test]
    fn ruby_path_prepends_contains_bin_and_shims() {
        if std::env::var("HOME").is_err() {
            return;
        }
        let dep = Dependency::simple("ruby");
        let prepends = RubyModule.path_prepends(&dep);
        assert_eq!(prepends.len(), 2);
        assert!(prepends[0].ends_with("/bin"));
        assert!(prepends[1].ends_with("/shims"));
    }

    #[test]
    fn ruby_post_setup_no_gemfile_is_noop() {
        // A temp dir with no Gemfile — post_setup must return Ok without running bundle.
        let dir = std::env::temp_dir().join(format!("devy_ruby_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dep = Dependency::simple("ruby");
        assert!(RubyModule.post_setup(&dep, &dir).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ruby_resolved_version_winget_delegates_to_pm() {
        let pm = MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        let dep = Dependency::simple("ruby");
        assert!(RubyModule.resolved_version(&pm, &dep).is_ok());
    }
}
