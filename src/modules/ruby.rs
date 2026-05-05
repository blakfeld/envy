use anyhow::Result;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, extra_strs, pm_dep, ruby_pkg, run_cmd};

pub struct RubyModule;

impl Module for RubyModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, ruby_pkg(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, ruby_pkg(pm)))?;

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
    use crate::package_manager::MockPackageManager;

    #[test]
    fn ruby_module_is_not_a_service() {
        assert!(!RubyModule.is_service());
    }

    #[test]
    fn ruby_is_installed_checks_ruby_formula() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("ruby");
        assert!(!RubyModule.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn ruby_is_installed_true() {
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            RubyModule
                .is_installed(&pm, &Dependency::simple("ruby"))
                .unwrap()
        );
    }

    #[test]
    fn ruby_install_without_gems_does_not_run_gem() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("ruby");
        assert!(RubyModule.install(&pm, &dep).is_ok());
    }

    #[test]
    fn ruby_install_propagates_pm_error() {
        let pm = MockPackageManager {
            install_fails: true,
            ..Default::default()
        };
        assert!(
            RubyModule
                .install(&pm, &Dependency::simple("ruby"))
                .is_err()
        );
    }
}
