use anyhow::Result;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, extra_strs, node_pkg, pm_dep, run_cmd};

pub struct TypeScriptModule;

impl Module for TypeScriptModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, node_pkg(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, node_pkg(pm)))?;

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
    use crate::package_manager::MockPackageManager;

    #[test]
    fn typescript_module_is_not_a_service() {
        assert!(!TypeScriptModule.is_service());
    }

    #[test]
    fn typescript_is_installed_checks_node_formula() {
        let pm = MockPackageManager { installed: true, ..Default::default() };
        let dep = Dependency::simple("typescript");
        assert!(TypeScriptModule.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn typescript_source_is_homebrew() {
        assert_eq!(TypeScriptModule.source(), "homebrew");
    }

    #[test]
    fn typescript_is_installed_false() {
        let pm = MockPackageManager::default();
        assert!(!TypeScriptModule.is_installed(&pm, &Dependency::simple("typescript")).unwrap());
    }

    #[test]
    fn typescript_install_propagates_pm_error() {
        // When pm.install_package fails, install must return Err (not Ok(())).
        let pm = MockPackageManager { install_fails: true, ..Default::default() };
        let dep = Dependency::simple("typescript");
        assert!(TypeScriptModule.install(&pm, &dep).is_err());
    }

    #[test]
    fn typescript_install_calls_install_package() {
        // Verify install_package is actually called (kills replace install -> Ok(())).
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("typescript");
        // install calls pm.install_package then run_cmd("npm", ...).
        // pm.install_package succeeds; npm may fail if not on PATH — both are Err-worthy.
        // The important thing: with mutation Ok(()), install_package would NOT be called.
        let _ = TypeScriptModule.install(&pm, &dep);
        // install_package must have been called (even if npm then fails)
        assert!(
            !pm.installed_packages.borrow().is_empty(),
            "install must call pm.install_package"
        );
    }
}
