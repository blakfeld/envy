use anyhow::Result;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, extra_strs, run_cmd};

pub struct NodeModule;

impl Module for NodeModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(dep)
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(dep)?;

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
    fn node_module_is_not_a_service() {
        assert!(!NodeModule.is_service());
    }

    #[test]
    fn node_is_installed_delegates_to_pm() {
        let pm = MockPm { installed: true };
        let dep = Dependency::simple("node");
        assert!(NodeModule.is_installed(&pm, &dep).unwrap());

        let pm2 = MockPm { installed: false };
        assert!(!NodeModule.is_installed(&pm2, &dep).unwrap());
    }

    #[test]
    fn node_install_without_global_packages_does_not_run_npm() {
        // install_package succeeds; no global_packages key means no npm call.
        let pm = MockPm { installed: false };
        let dep = Dependency::simple("node");
        // This would run `npm install -g` if packages were present; with an empty
        // list it returns Ok without spawning npm.
        assert!(NodeModule.install(&pm, &dep).is_ok());
    }
}
