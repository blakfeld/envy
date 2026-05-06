use anyhow::Result;
use std::collections::HashMap;
use std::process::Command;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep};

pub struct JavaModule;

fn pkg_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "apt" => "default-jdk",
        "winget" => "Microsoft.OpenJDK.21",
        _ => "openjdk",
    }
}

fn detect_java_home() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("/usr/libexec/java_home").output().ok()?;
        if out.status.success() {
            let path = String::from_utf8(out.stdout).ok()?.trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
        None
    }
    #[cfg(target_os = "linux")]
    {
        for candidate in &[
            "/usr/lib/jvm/default-java",
            "/usr/lib/jvm/java-21-openjdk-amd64",
            "/usr/lib/jvm/java-17-openjdk-amd64",
            "/usr/lib/jvm/java-11-openjdk-amd64",
        ] {
            if std::path::Path::new(candidate).exists() {
                return Some(candidate.to_string());
            }
        }
        None
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("JAVA_HOME").ok()
    }
}

impl Module for JavaModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, pkg_name(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, pkg_name(pm)))
    }

    fn env_vars(&self, _dep: &Dependency) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        if let Some(home) = detect_java_home() {
            vars.insert("JAVA_HOME".into(), home);
        }
        vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;

    #[test]
    fn java_is_not_a_service() {
        assert!(!JavaModule.is_service());
    }

    #[test]
    fn pkg_name_apt() {
        let pm = MockPackageManager {
            name: "apt",
            ..Default::default()
        };
        assert_eq!(pkg_name(&pm), "default-jdk");
    }

    #[test]
    fn pkg_name_winget() {
        let pm = MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(pkg_name(&pm), "Microsoft.OpenJDK.21");
    }

    #[test]
    fn pkg_name_brew() {
        let pm = MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(pkg_name(&pm), "openjdk");
    }

    #[test]
    fn java_is_installed_delegates_to_pm() {
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            JavaModule
                .is_installed(&pm, &Dependency::simple("java"))
                .unwrap()
        );
    }

    #[test]
    fn java_not_installed_when_pm_reports_false() {
        let pm = MockPackageManager::default();
        assert!(
            !JavaModule
                .is_installed(&pm, &Dependency::simple("java"))
                .unwrap()
        );
    }

    #[test]
    fn java_install_propagates_pm_error() {
        let pm = MockPackageManager {
            install_fails: true,
            ..Default::default()
        };
        assert!(
            JavaModule
                .install(&pm, &Dependency::simple("java"))
                .is_err()
        );
    }

    #[test]
    fn java_env_vars_returns_map() {
        let dep = Dependency::simple("java");
        // env_vars must not panic; JAVA_HOME presence depends on the environment.
        let vars = JavaModule.env_vars(&dep);
        if let Some(home) = vars.get("JAVA_HOME") {
            assert!(!home.is_empty(), "JAVA_HOME must not be empty if present");
        }
    }

    #[test]
    fn java_path_prepends_is_empty() {
        let dep = Dependency::simple("java");
        assert!(JavaModule.path_prepends(&dep).is_empty());
    }
}
