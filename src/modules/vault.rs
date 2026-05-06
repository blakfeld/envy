use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep};

pub struct VaultModule;

// apt requires the HashiCorp apt repository; see https://developer.hashicorp.com/vault/install
fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "Hashicorp.Vault",
        _ => "vault",
    }
}

fn port(dep: &Dependency) -> u16 {
    dep.extra
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(8200) as u16
}

fn dev_mode(dep: &Dependency) -> bool {
    dep.extra
        .get("dev_mode")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

impl Module for VaultModule {
    fn is_service(&self) -> bool {
        true
    }

    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, package_name(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, package_name(pm)))
    }

    fn is_running(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_service_running(&self.service_name(dep))
    }

    fn start(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.start_service(&self.service_name(dep))
    }

    fn stop(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.stop_service(&self.service_name(dep))
    }

    fn health_check(&self, dep: &Dependency) -> Result<()> {
        let p = port(dep);
        let url = format!("http://127.0.0.1:{p}/v1/sys/health");
        // Vault returns non-200 codes for standby/sealed states, all of which
        // still indicate the process is up and responding.
        match ureq::get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .call()
        {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(_, _)) => Ok(()),
            Err(e) => Err(e).with_context(|| format!("Vault not reachable on port {p}")),
        }
    }

    fn env_vars(&self, dep: &Dependency) -> HashMap<String, String> {
        let p = port(dep);
        let mut vars = HashMap::new();
        vars.insert("VAULT_ADDR".into(), format!("http://127.0.0.1:{p}"));
        if dev_mode(dep) {
            // Conventional root token for dev mode; users can override in devy.yml environment.
            vars.insert("VAULT_TOKEN".into(), "root".into());
        }
        vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn dep_with_port(port: u64) -> Dependency {
        let mut extra = HashMap::new();
        extra.insert("port".into(), serde_yaml::Value::Number(port.into()));
        Dependency::with_extra("vault", extra)
    }

    #[test]
    fn vault_module_is_service() {
        assert!(VaultModule.is_service());
    }

    #[test]
    fn port_defaults_to_8200() {
        let dep = Dependency::simple("vault");
        assert_eq!(port(&dep), 8200);
    }

    #[test]
    fn port_reads_custom_value() {
        let dep = dep_with_port(8201);
        assert_eq!(port(&dep), 8201);
    }

    #[test]
    fn vault_health_check_fails_on_unused_port() {
        let dep = dep_with_port(19986);
        let err = VaultModule.health_check(&dep).unwrap_err();
        assert!(err.to_string().contains("19986"));
    }

    #[test]
    fn dev_mode_defaults_to_false() {
        let dep = Dependency::simple("vault");
        assert!(!dev_mode(&dep));
    }

    #[test]
    fn dev_mode_reads_true() {
        let mut extra = HashMap::new();
        extra.insert("dev_mode".into(), serde_yaml::Value::Bool(true));
        let dep = Dependency::with_extra("vault", extra);
        assert!(dev_mode(&dep));
    }

    #[test]
    fn env_vars_always_includes_vault_addr() {
        let dep = Dependency::simple("vault");
        let vars = VaultModule.env_vars(&dep);
        assert_eq!(
            vars.get("VAULT_ADDR").map(|s| s.as_str()),
            Some("http://127.0.0.1:8200")
        );
        assert!(!vars.contains_key("VAULT_TOKEN"));
    }

    #[test]
    fn env_vars_includes_vault_token_in_dev_mode() {
        let mut extra = HashMap::new();
        extra.insert("dev_mode".into(), serde_yaml::Value::Bool(true));
        let dep = Dependency::with_extra("vault", extra);
        let vars = VaultModule.env_vars(&dep);
        assert_eq!(vars.get("VAULT_TOKEN").map(|s| s.as_str()), Some("root"));
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "Hashicorp.Vault");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "vault");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            VaultModule
                .is_installed(&pm, &Dependency::simple("vault"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !VaultModule
                .is_installed(&pm, &Dependency::simple("vault"))
                .unwrap()
        );
    }

    #[test]
    fn install_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            install_fails: true,
            ..Default::default()
        };
        assert!(
            VaultModule
                .install(&pm, &Dependency::simple("vault"))
                .is_err()
        );
    }

    #[test]
    fn is_running_true() {
        let pm = crate::package_manager::MockPackageManager {
            service_running: true,
            ..Default::default()
        };
        assert!(
            VaultModule
                .is_running(&pm, &Dependency::simple("vault"))
                .unwrap()
        );
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !VaultModule
                .is_running(&pm, &Dependency::simple("vault"))
                .unwrap()
        );
    }

    #[test]
    fn start_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(VaultModule.start(&pm, &Dependency::simple("vault")).is_ok());
    }

    #[test]
    fn start_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            start_service_fails: true,
            ..Default::default()
        };
        assert!(
            VaultModule
                .start(&pm, &Dependency::simple("vault"))
                .is_err()
        );
    }

    #[test]
    fn stop_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(VaultModule.stop(&pm, &Dependency::simple("vault")).is_ok());
    }

    #[test]
    fn stop_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            stop_service_fails: true,
            ..Default::default()
        };
        assert!(VaultModule.stop(&pm, &Dependency::simple("vault")).is_err());
    }

    #[test]
    fn env_vars_addr_uses_custom_port() {
        let dep = dep_with_port(8201);
        let vars = VaultModule.env_vars(&dep);
        assert_eq!(
            vars.get("VAULT_ADDR").map(|s| s.as_str()),
            Some("http://127.0.0.1:8201")
        );
    }
}
