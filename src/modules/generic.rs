use anyhow::Result;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::Module;

pub struct GenericModule;

impl Module for GenericModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(dep)
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(dep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;

    #[test]
    fn generic_module_is_not_a_service() {
        assert!(!GenericModule.is_service());
    }

    #[test]
    fn generic_is_installed_delegates_to_pm_true() {
        let pm = MockPackageManager { installed: true, ..Default::default() };
        let dep = Dependency::simple("jq");
        assert!(GenericModule.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn generic_is_installed_delegates_to_pm_false() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("jq");
        assert!(!GenericModule.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn generic_install_delegates_to_pm() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("jq");
        assert!(GenericModule.install(&pm, &dep).is_ok());
    }

    #[test]
    fn generic_install_propagates_pm_error() {
        let pm = MockPackageManager { install_fails: true, ..Default::default() };
        let dep = Dependency::simple("jq");
        assert!(GenericModule.install(&pm, &dep).is_err());
    }
}
