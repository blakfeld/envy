use anyhow::Result;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, brew_dep, extra_strs, run_cmd};

pub struct TypeScriptModule;

impl Module for TypeScriptModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&brew_dep(dep, "node"))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&brew_dep(dep, "node"))?;

        let mut globals = vec!["typescript".to_string()];
        globals.extend(extra_strs(dep, "global_packages"));

        let refs: Vec<&str> = globals.iter().map(String::as_str).collect();
        let mut args = vec!["install", "-g"];
        args.extend_from_slice(&refs);
        run_cmd("npm", &args)?;

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
    fn typescript_module_is_not_a_service() {
        assert!(!TypeScriptModule.is_service());
    }

    #[test]
    fn typescript_is_installed_checks_node_formula() {
        let pm = MockPm { installed: true };
        let dep = Dependency::simple("typescript");
        assert!(TypeScriptModule.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn typescript_source_is_homebrew() {
        assert_eq!(TypeScriptModule.source(), "homebrew");
    }
}
