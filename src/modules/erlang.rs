use anyhow::Result;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep};

pub struct ErlangModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "Erlang-Solutions.Erlang",
        _ => "erlang",
    }
}

impl Module for ErlangModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, package_name(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, package_name(pm)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;

    #[test]
    fn erlang_module_is_not_a_service() {
        assert!(!ErlangModule.is_service());
    }

    #[test]
    fn package_name_winget() {
        let pm = MockPackageManager { name: "winget", ..Default::default() };
        assert_eq!(package_name(&pm), "Erlang-Solutions.Erlang");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = MockPackageManager { name: "brew", ..Default::default() };
        assert_eq!(package_name(&pm), "erlang");
    }

    #[test]
    fn package_name_apt_default() {
        let pm = MockPackageManager { name: "apt", ..Default::default() };
        assert_eq!(package_name(&pm), "erlang");
    }

    #[test]
    fn is_installed_true() {
        let pm = MockPackageManager { installed: true, ..Default::default() };
        assert!(ErlangModule.is_installed(&pm, &Dependency::simple("erlang")).unwrap());
    }

    #[test]
    fn is_installed_false() {
        let pm = MockPackageManager::default();
        assert!(!ErlangModule.is_installed(&pm, &Dependency::simple("erlang")).unwrap());
    }

    #[test]
    fn install_propagates_pm_error() {
        let pm = MockPackageManager { install_fails: true, ..Default::default() };
        assert!(ErlangModule.install(&pm, &Dependency::simple("erlang")).is_err());
    }
}
