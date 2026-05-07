use anyhow::Result;
use std::collections::HashMap;
use std::process::Command;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep};

pub struct JavaModule;

fn winget_package_id(dep: &Dependency) -> String {
    let major = dep
        .version
        .as_deref()
        .and_then(|v| v.split('.').next())
        .unwrap_or("21");
    format!("Microsoft.OpenJDK.{major}")
}

fn pkg_name(pm: &dyn PackageManager, dep: &Dependency) -> String {
    match pm.name() {
        "apt" => "default-jdk".into(),
        "winget" => winget_package_id(dep),
        _ => "openjdk".into(),
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
        // Prefer the distro-managed symlink first (works on any arch/version).
        if std::path::Path::new("/usr/lib/jvm/default-java").exists() {
            return Some("/usr/lib/jvm/default-java".into());
        }
        // Fall back to deriving JAVA_HOME from the `java` binary on PATH.
        if let Ok(java) = which::which("java") {
            if let Ok(resolved) = java.canonicalize() {
                // java is typically at $JAVA_HOME/bin/java — go up two levels.
                if let Some(home) = resolved.ancestors().nth(2) {
                    return Some(home.display().to_string());
                }
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
        pm.is_package_installed(&pm_dep(dep, &pkg_name(pm, dep)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, &pkg_name(pm, dep)))
    }

    fn env_vars(
        &self,
        _dep: &Dependency,
        _project_root: &std::path::Path,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        if let Some(home) = detect_java_home() {
            vars.insert("JAVA_HOME".into(), home);
        }
        vars
    }

    fn path_prepends(&self, _dep: &Dependency, _project_root: &std::path::Path) -> Vec<String> {
        detect_java_home()
            .map(|home| vec![format!("{home}/bin")])
            .unwrap_or_default()
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
        assert_eq!(pkg_name(&pm, &Dependency::simple("java")), "default-jdk");
    }

    #[test]
    fn pkg_name_winget_defaults_to_21() {
        let pm = MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(
            pkg_name(&pm, &Dependency::simple("java")),
            "Microsoft.OpenJDK.21"
        );
    }

    #[test]
    fn pkg_name_winget_uses_version_field() {
        let pm = MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        let dep = Dependency {
            version: Some("17".into()),
            ..Dependency::simple("java")
        };
        assert_eq!(pkg_name(&pm, &dep), "Microsoft.OpenJDK.17");
    }

    #[test]
    fn pkg_name_winget_uses_major_from_semver() {
        let pm = MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        let dep = Dependency {
            version: Some("21.0.2".into()),
            ..Dependency::simple("java")
        };
        assert_eq!(pkg_name(&pm, &dep), "Microsoft.OpenJDK.21");
    }

    #[test]
    fn pkg_name_brew() {
        let pm = MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(pkg_name(&pm, &Dependency::simple("java")), "openjdk");
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
        let vars = JavaModule.env_vars(&dep, std::path::Path::new("/tmp"));
        if let Some(home) = vars.get("JAVA_HOME") {
            assert!(!home.is_empty(), "JAVA_HOME must not be empty if present");
        }
    }

    #[test]
    fn java_path_prepends_contains_java_home_bin_when_detected() {
        let dep = Dependency::simple("java");
        let prepends = JavaModule.path_prepends(&dep, std::path::Path::new("/tmp"));
        if detect_java_home().is_some() {
            assert_eq!(
                prepends.len(),
                1,
                "must have exactly one PATH entry when JAVA_HOME is detected"
            );
            assert!(
                prepends[0].ends_with("/bin"),
                "PATH entry must point to $JAVA_HOME/bin"
            );
        } else {
            assert!(
                prepends.is_empty(),
                "must be empty when JAVA_HOME cannot be detected"
            );
        }
    }
}
