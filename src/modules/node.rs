use anyhow::Result;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, extra_strs, node_pkg, pm_dep, run_cmd};

pub struct NodeModule;

impl Module for NodeModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, node_pkg(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, node_pkg(pm)))?;

        let packages = extra_strs(dep, "global_packages");
        if !packages.is_empty() {
            let refs: Vec<&str> = packages.iter().map(String::as_str).collect();
            let mut args = vec!["install", "-g"];
            args.extend_from_slice(&refs);
            run_cmd("npm", &args)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;

    #[test]
    fn node_module_is_not_a_service() {
        assert!(!NodeModule.is_service());
    }

    #[test]
    fn node_is_installed_delegates_to_pm() {
        let pm = MockPackageManager { installed: true, ..Default::default() };
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
        let pm = MockPackageManager { install_fails: true, ..Default::default() };
        assert!(NodeModule.install(&pm, &Dependency::simple("node")).is_err());
    }
}
