use anyhow::Result;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, brew_dep, extra_strs, run_cmd};

pub struct RubyModule;

impl Module for RubyModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&brew_dep(dep, "ruby"))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&brew_dep(dep, "ruby"))?;

        let gems = extra_strs(dep, "gems");
        if !gems.is_empty() {
            let refs: Vec<&str> = gems.iter().map(String::as_str).collect();
            let mut args = vec!["install"];
            args.extend_from_slice(&refs);
            run_cmd("gem", &args)?;
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
    fn ruby_module_is_not_a_service() {
        assert!(!RubyModule.is_service());
    }

    #[test]
    fn ruby_is_installed_checks_ruby_formula() {
        // MockPm reports nothing installed; is_installed delegates to pm with "ruby" formula.
        let pm = MockPm { installed: false };
        let dep = Dependency::simple("ruby");
        assert!(!RubyModule.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn ruby_install_without_gems_does_not_run_gem() {
        let pm = MockPm { installed: false };
        let dep = Dependency::simple("ruby");
        assert!(RubyModule.install(&pm, &dep).is_ok());
    }
}
