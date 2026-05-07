use anyhow::Result;
use std::path::Path;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, extra_strs, node_pkg, pm_dep, run_cmd};

pub struct NodeModule;

impl Module for NodeModule {
    fn known_extra_keys(&self) -> Option<&'static [&'static str]> {
        Some(&["global_packages"])
    }

    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, node_pkg(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, node_pkg(pm)))
    }

    fn post_setup(
        &self,
        dep: &Dependency,
        _pm: &dyn PackageManager,
        project_root: &Path,
    ) -> Result<()> {
        let packages = extra_strs(dep, "global_packages");
        if packages.is_empty() {
            return Ok(());
        }
        let stamp = project_root.join(".devy_node_global_stamp");
        let current = packages.join("\n");
        if std::fs::read_to_string(&stamp).ok().as_deref() == Some(current.as_str()) {
            return Ok(());
        }
        let refs: Vec<&str> = packages.iter().map(String::as_str).collect();
        let mut args = vec!["install", "-g"];
        args.extend_from_slice(&refs);
        run_cmd("npm", &args)?;
        let _ = std::fs::write(&stamp, &current);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;
    use std::collections::HashMap;

    fn dep_with_global_packages(pkgs: &[&str]) -> Dependency {
        let mut extra = HashMap::new();
        extra.insert(
            "global_packages".into(),
            crate::config::ExtraValue::Sequence(
                pkgs.iter()
                    .map(|s| crate::config::ExtraValue::String((*s).into()))
                    .collect(),
            ),
        );
        Dependency::with_extra("node", extra)
    }

    #[test]
    fn node_module_is_not_a_service() {
        assert!(!NodeModule.is_service());
    }

    #[test]
    fn node_is_installed_delegates_to_pm() {
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        let dep = Dependency::simple("node");
        assert!(NodeModule.is_installed(&pm, &dep).unwrap());

        let pm2 = MockPackageManager::default();
        assert!(!NodeModule.is_installed(&pm2, &dep).unwrap());
    }

    #[test]
    fn node_install_without_global_packages_does_not_run_npm() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("node");
        assert!(NodeModule.install(&pm, &dep).is_ok());
    }

    #[test]
    fn node_install_propagates_pm_error() {
        let pm = MockPackageManager {
            install_fails: true,
            ..Default::default()
        };
        assert!(
            NodeModule
                .install(&pm, &Dependency::simple("node"))
                .is_err()
        );
    }

    #[test]
    fn node_post_setup_no_global_packages_is_noop() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("node");
        assert!(
            NodeModule
                .post_setup(&dep, &pm, std::path::Path::new("/tmp"))
                .is_ok(),
            "post_setup with no global_packages must return Ok"
        );
    }

    #[test]
    fn node_post_setup_skips_npm_when_stamp_matches() {
        let dir = crate::test_support::tmp_dir();
        let dep = dep_with_global_packages(&["typescript"]);
        let stamp = dir.join(".devy_node_global_stamp");
        // Write a stamp that already matches the package list.
        std::fs::write(&stamp, "typescript").unwrap();
        let pm = MockPackageManager::default();
        // If the stamp check is bypassed, `npm` would be invoked and likely fail,
        // returning Err. A successful return proves the stamp short-circuited.
        let result = NodeModule.post_setup(&dep, &pm, &dir);
        assert!(
            result.is_ok(),
            "post_setup must skip npm when stamp matches"
        );
        assert_eq!(
            std::fs::read_to_string(&stamp).unwrap(),
            "typescript",
            "stamp must be unchanged when skipped"
        );
    }

    #[test]
    fn node_post_setup_stamp_matches_multiple_packages() {
        let dir = crate::test_support::tmp_dir();
        let dep = dep_with_global_packages(&["typescript", "eslint"]);
        let stamp = dir.join(".devy_node_global_stamp");
        std::fs::write(&stamp, "typescript\neslint").unwrap();
        let pm = MockPackageManager::default();
        let result = NodeModule.post_setup(&dep, &pm, &dir);
        assert!(
            result.is_ok(),
            "post_setup must skip npm when stamp matches multiple packages"
        );
    }
}
