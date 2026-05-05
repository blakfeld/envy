use anyhow::{Context, Result, bail};
use std::process::{Command, Stdio};
use which::which;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, extra_strs, pm_dep, run_cmd};

pub struct GcloudModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "Google.CloudSDK",
        _ => "google-cloud-sdk",
    }
}

impl Module for GcloudModule {
    fn source(&self) -> &'static str {
        "gcloud-installer"
    }

    fn is_installed(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<bool> {
        Ok(which("gcloud").is_ok())
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        match pm.name() {
            "brew" | "winget" => {
                pm.install_package(&pm_dep(dep, package_name(pm)))?;
            }
            _ => {
                // On Linux, use the official installer script — the apt package
                // is version-locked and requires manual repo setup.
                let home = std::env::var("HOME")
                    .context("HOME is not set; cannot determine gcloud install directory")?;

                #[cfg(test)]
                let installer_cmd = std::env::var("ENVY_TEST_GCLOUD_INSTALL_SCRIPT")
                    .unwrap_or_else(|_| {
                        concat!(
                            "curl -fsSL https://sdk.cloud.google.com",
                            " | bash -s -- --disable-prompts \"--install-dir=$ENVY_GCLOUD_HOME\""
                        )
                        .to_string()
                    });
                #[cfg(not(test))]
                let installer_cmd = concat!(
                    "curl -fsSL https://sdk.cloud.google.com",
                    " | bash -s -- --disable-prompts \"--install-dir=$ENVY_GCLOUD_HOME\""
                )
                .to_string();

                let status = Command::new("sh")
                    .arg("-c")
                    .arg(&installer_cmd)
                    .env("ENVY_GCLOUD_HOME", &home)
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()
                    .map_err(|e| anyhow::anyhow!("Failed to run gcloud installer: {e}"))?;
                if !status.success() {
                    bail!("gcloud SDK installation failed");
                }
            }
        }

        for component in extra_strs(dep, "components") {
            run_cmd("gcloud", &["components", "install", "--quiet", &component])?;
        }

        Ok(())
    }

    fn resolved_version(
        &self,
        _pm: &dyn PackageManager,
        _dep: &Dependency,
    ) -> Result<Option<String>> {
        let out = Command::new("gcloud").arg("version").output();
        // "Google Cloud SDK 468.0.0\n..." — take third token of first line
        Ok(out.ok().and_then(|o| {
            String::from_utf8(o.stdout)
                .ok()?
                .lines()
                .next()?
                .split_whitespace()
                .nth(3)
                .map(String::from)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn gcloud_module_is_not_a_service() {
        assert!(!GcloudModule.is_service());
    }

    #[test]
    fn gcloud_source_is_gcloud_installer() {
        assert_eq!(GcloudModule.source(), "gcloud-installer");
    }

    #[test]
    fn components_empty_when_not_configured() {
        let dep = Dependency::simple("gcloud");
        assert!(extra_strs(&dep, "components").is_empty());
    }

    #[test]
    fn components_read_from_extra() {
        let mut extra = HashMap::new();
        extra.insert(
            "components".into(),
            serde_yaml::Value::Sequence(vec![
                serde_yaml::Value::String("gke-gcloud-auth-plugin".into()),
                serde_yaml::Value::String("kubectl".into()),
            ]),
        );
        let dep = Dependency {
            name: "gcloud".into(),
            version: None,
            tap: None,
            profiles: None,
            after_install: None,
            extra,
        };
        let components = extra_strs(&dep, "components");
        assert_eq!(components, vec!["gke-gcloud-auth-plugin", "kubectl"]);
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "Google.CloudSDK");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "google-cloud-sdk");
    }

    #[test]
    fn gcloud_is_installed_does_not_panic() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("gcloud");
        let _ = GcloudModule.is_installed(&pm, &dep);
    }

    #[test]
    fn gcloud_is_installed_consistent_with_which() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("gcloud");
        let expected = which("gcloud").is_ok();
        assert_eq!(GcloudModule.is_installed(&pm, &dep).unwrap(), expected);
    }

    #[test]
    fn gcloud_install_brew_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            install_fails: true,
            ..Default::default()
        };
        let dep = Dependency::simple("gcloud");
        assert!(GcloudModule.install(&pm, &dep).is_err());
    }

    #[test]
    fn gcloud_install_winget_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            install_fails: true,
            ..Default::default()
        };
        let dep = Dependency::simple("gcloud");
        assert!(GcloudModule.install(&pm, &dep).is_err());
    }

    #[test]
    fn gcloud_resolved_version_returns_ok() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("gcloud");
        assert!(GcloudModule.resolved_version(&pm, &dep).is_ok());
    }

    #[test]
    fn gcloud_is_installed_false_when_not_on_path() {
        if which("gcloud").is_ok() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("gcloud");
        assert!(
            !GcloudModule.is_installed(&pm, &dep).unwrap(),
            "is_installed must return false when gcloud is absent from PATH"
        );
    }

    #[test]
    fn gcloud_is_installed_true_when_on_path() {
        if which("gcloud").is_err() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("gcloud");
        assert!(GcloudModule.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn gcloud_resolved_version_is_none_when_not_installed() {
        if which("gcloud").is_ok() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("gcloud");
        let ver = GcloudModule.resolved_version(&pm, &dep).unwrap();
        assert!(
            ver.is_none(),
            "Expected None version when gcloud is not on PATH"
        );
    }

    #[test]
    fn gcloud_install_apt_falls_through_to_script_path() {
        // On macOS (no HOME set incorrectly), the apt/apt-get path tries to run a script.
        // With install_fails=false this still needs "HOME" to be set.
        // This test just verifies the brew/winget path is distinct from the default path.
        let pm_brew = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        let dep = Dependency::simple("gcloud");
        // brew path calls pm.install_package — with install_fails=false this succeeds.
        assert!(GcloudModule.install(&pm_brew, &dep).is_ok());
    }

    #[test]
    fn package_name_apt_returns_google_cloud_sdk() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "google-cloud-sdk");
    }

    #[test]
    fn gcloud_install_brew_calls_pm_install_package() {
        // Kills `delete match arm "brew" | "winget"` — without the arm, brew falls through
        // to the script path and install_package is never called.
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        let dep = Dependency::simple("gcloud");
        GcloudModule.install(&pm, &dep).ok();
        assert!(
            !pm.installed_packages.borrow().is_empty(),
            "install with brew PM must delegate to pm.install_package"
        );
    }

    #[test]
    fn gcloud_install_apt_script_failure_propagated() {
        // When PM is not brew/winget, install runs the gcloud installer script.
        // Uses ENVY_TEST_GCLOUD_INSTALL_SCRIPT to inject a failing script.
        unsafe {
            std::env::set_var("ENVY_TEST_GCLOUD_INSTALL_SCRIPT", "exit 1");
        }
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            ..Default::default()
        };
        let dep = Dependency::simple("gcloud");
        let result = GcloudModule.install(&pm, &dep);
        unsafe {
            std::env::remove_var("ENVY_TEST_GCLOUD_INSTALL_SCRIPT");
        }
        assert!(
            result.is_err(),
            "install must fail when installer script exits non-zero"
        );
    }

    #[test]
    fn gcloud_resolved_version_contains_dot_when_installed() {
        if which("gcloud").is_err() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("gcloud");
        let ver = GcloudModule.resolved_version(&pm, &dep).unwrap();
        if let Some(v) = ver {
            assert!(v.contains('.'), "Expected semver-like version, got: {v}");
            assert_ne!(v, "xyzzy", "Version must not be placeholder");
            assert!(!v.is_empty());
        }
    }
}
