use anyhow::Result;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep};

pub struct DotnetModule;

/// Extracts the major version number from a version string like "8", "8.0", or "8.0.1".
fn major_version(dep: &Dependency) -> u32 {
    dep.version
        .as_deref()
        .and_then(|v| v.split('.').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(8)
}

fn package_name(pm: &dyn PackageManager, dep: &Dependency) -> String {
    let major = major_version(dep);
    match pm.name() {
        "apt" => format!("dotnet-sdk-{major}.0"),
        "winget" => format!("Microsoft.DotNet.SDK.{major}"),
        _ => "dotnet".to_string(),
    }
}

impl Module for DotnetModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        let name = package_name(pm, dep);
        pm.is_package_installed(&pm_dep(dep, &name))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        let name = package_name(pm, dep);
        pm.install_package(&pm_dep(dep, &name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep_with_version(v: &str) -> Dependency {
        let mut dep = Dependency::simple("dotnet");
        dep.version = Some(v.into());
        dep
    }

    #[test]
    fn dotnet_module_is_not_a_service() {
        assert!(!DotnetModule.is_service());
    }

    #[test]
    fn major_version_defaults_to_8() {
        let dep = Dependency::simple("dotnet");
        assert_eq!(major_version(&dep), 8);
    }

    #[test]
    fn major_version_from_bare_number() {
        assert_eq!(major_version(&dep_with_version("7")), 7);
    }

    #[test]
    fn major_version_from_full_semver() {
        assert_eq!(major_version(&dep_with_version("6.0.1")), 6);
    }

    #[test]
    fn apt_package_name_includes_major_version() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            ..Default::default()
        };
        assert_eq!(package_name(&pm, &dep_with_version("8")), "dotnet-sdk-8.0");
        assert_eq!(package_name(&pm, &dep_with_version("6")), "dotnet-sdk-6.0");
    }

    #[test]
    fn winget_package_name_includes_major_version() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(
            package_name(&pm, &dep_with_version("8")),
            "Microsoft.DotNet.SDK.8"
        );
    }

    #[test]
    fn brew_package_name_is_dotnet() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm, &dep_with_version("9")), "dotnet");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            DotnetModule
                .is_installed(&pm, &Dependency::simple("dotnet"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !DotnetModule
                .is_installed(&pm, &Dependency::simple("dotnet"))
                .unwrap()
        );
    }

    #[test]
    fn install_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            install_fails: true,
            ..Default::default()
        };
        assert!(
            DotnetModule
                .install(&pm, &Dependency::simple("dotnet"))
                .is_err()
        );
    }
}
