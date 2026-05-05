use anyhow::Result;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use crate::output;

use super::{Module, pm_dep};

pub struct ElixirModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "Erlang-Solutions.Elixir",
        _ => "elixir",
    }
}

impl Module for ElixirModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, package_name(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        // Elixir requires Erlang — install it first if not already present.
        let erlang_dep = Dependency::simple("erlang");
        let erlang = super::get("erlang");
        if !erlang.is_installed(pm, &erlang_dep)? {
            output::step("Installing erlang (required by elixir)");
            erlang.install(pm, &erlang_dep)?;
            output::success("erlang installed");
        }
        pm.install_package(&pm_dep(dep, package_name(pm)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;

    #[test]
    fn elixir_module_is_not_a_service() {
        assert!(!ElixirModule.is_service());
    }

    #[test]
    fn package_name_winget() {
        let pm = MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "Erlang-Solutions.Elixir");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "elixir");
    }

    #[test]
    fn is_installed_true() {
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            ElixirModule
                .is_installed(&pm, &Dependency::simple("elixir"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = MockPackageManager::default();
        assert!(
            !ElixirModule
                .is_installed(&pm, &Dependency::simple("elixir"))
                .unwrap()
        );
    }

    #[test]
    fn install_installs_erlang_first_when_not_present() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("elixir");
        assert!(ElixirModule.install(&pm, &dep).is_ok());
    }

    #[test]
    fn install_skips_erlang_when_already_installed() {
        // With installed=true, erlang is already present — install should not try to install it.
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        let dep = Dependency::simple("elixir");
        assert!(ElixirModule.install(&pm, &dep).is_ok());
    }

    #[test]
    fn install_propagates_pm_error() {
        let pm = MockPackageManager {
            install_fails: true,
            ..Default::default()
        };
        let dep = Dependency::simple("elixir");
        assert!(ElixirModule.install(&pm, &dep).is_err());
    }

    #[test]
    fn install_propagates_erlang_pm_error_when_not_installed() {
        let pm = MockPackageManager {
            installed: false,
            install_fails: true,
            ..Default::default()
        };
        let dep = Dependency::simple("elixir");
        assert!(ElixirModule.install(&pm, &dep).is_err());
    }

    #[test]
    fn install_does_not_reinstall_erlang_when_already_installed() {
        // installed=true means is_package_installed always returns true (erlang already present).
        // With the correct `!erlang.is_installed()` guard, erlang install is skipped.
        // Only elixir's install_package call happens → install_count = 1.
        // With mutant (delete !): erlang IS installed → condition true → install erlang too →
        // install_count = 2.
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        ElixirModule
            .install(&pm, &Dependency::simple("elixir"))
            .unwrap();
        assert_eq!(
            pm.installed_packages.borrow().len(),
            1,
            "Only elixir should be installed when erlang is already present; got: {:?}",
            pm.installed_packages.borrow()
        );
    }

    #[test]
    fn install_installs_erlang_then_elixir_when_neither_present() {
        let pm = MockPackageManager {
            installed: false,
            ..Default::default()
        };
        ElixirModule
            .install(&pm, &Dependency::simple("elixir"))
            .unwrap();
        let pkgs = pm.installed_packages.borrow();
        assert_eq!(
            pkgs.len(),
            2,
            "Expected erlang + elixir to be installed, got: {pkgs:?}"
        );
    }
}
