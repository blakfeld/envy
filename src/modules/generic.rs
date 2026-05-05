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
    use anyhow::Result;

    struct MockPm {
        installed: bool,
    }
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
            Ok(self.installed)
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

    #[test]
    fn generic_module_is_not_a_service() {
        assert!(!GenericModule.is_service());
    }

    #[test]
    fn generic_is_installed_delegates_to_pm_true() {
        let pm = MockPm { installed: true };
        let dep = Dependency::simple("jq");
        assert!(GenericModule.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn generic_is_installed_delegates_to_pm_false() {
        let pm = MockPm { installed: false };
        let dep = Dependency::simple("jq");
        assert!(!GenericModule.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn generic_install_delegates_to_pm() {
        let pm = MockPm { installed: false };
        let dep = Dependency::simple("jq");
        assert!(GenericModule.install(&pm, &dep).is_ok());
    }
}
