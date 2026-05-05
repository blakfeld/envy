use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use crate::output;

use super::{Module, pm_dep};

pub struct MeilisearchModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "Meilisearch.Meilisearch",
        _ => "meilisearch",
    }
}

fn port(dep: &Dependency) -> u16 {
    dep.extra
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(7700) as u16
}

impl Module for MeilisearchModule {
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
        let url = format!("http://127.0.0.1:{p}/health");
        ureq::get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .call()
            .with_context(|| format!("Meilisearch not reachable on port {p}"))?;
        Ok(())
    }

    fn env_vars(&self, dep: &Dependency) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        if let Some(key) = dep.extra.get("master_key").and_then(|v| v.as_str()) {
            vars.insert("MEILI_MASTER_KEY".into(), key.to_string());
            output::warn(
                "meilisearch master_key written to plaintext shadowenv — consider using ejson secrets instead",
            );
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
        Dependency::with_extra("meilisearch", extra)
    }

    #[test]
    fn meilisearch_module_is_service() {
        assert!(MeilisearchModule.is_service());
    }

    #[test]
    fn port_defaults_to_7700() {
        let dep = Dependency::simple("meilisearch");
        assert_eq!(port(&dep), 7700);
    }

    #[test]
    fn port_reads_custom_value() {
        let dep = dep_with_port(7701);
        assert_eq!(port(&dep), 7701);
    }

    #[test]
    fn meilisearch_health_check_fails_on_unused_port() {
        let dep = dep_with_port(19990);
        let err = MeilisearchModule.health_check(&dep).unwrap_err();
        assert!(err.to_string().contains("19990"));
    }

    #[test]
    fn env_vars_empty_when_no_master_key() {
        let dep = Dependency::simple("meilisearch");
        assert!(MeilisearchModule.env_vars(&dep).is_empty());
    }

    #[test]
    fn env_vars_includes_master_key_when_configured() {
        let mut extra = HashMap::new();
        extra.insert(
            "master_key".into(),
            serde_yaml::Value::String("supersecret".into()),
        );
        let dep = Dependency::with_extra("meilisearch", extra);
        let vars = MeilisearchModule.env_vars(&dep);
        assert_eq!(
            vars.get("MEILI_MASTER_KEY").map(|s| s.as_str()),
            Some("supersecret")
        );
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "Meilisearch.Meilisearch");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "meilisearch");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            MeilisearchModule
                .is_installed(&pm, &Dependency::simple("meilisearch"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !MeilisearchModule
                .is_installed(&pm, &Dependency::simple("meilisearch"))
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
            MeilisearchModule
                .install(&pm, &Dependency::simple("meilisearch"))
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
            MeilisearchModule
                .is_running(&pm, &Dependency::simple("meilisearch"))
                .unwrap()
        );
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !MeilisearchModule
                .is_running(&pm, &Dependency::simple("meilisearch"))
                .unwrap()
        );
    }

    #[test]
    fn start_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            MeilisearchModule
                .start(&pm, &Dependency::simple("meilisearch"))
                .is_ok()
        );
    }

    #[test]
    fn start_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            start_service_fails: true,
            ..Default::default()
        };
        assert!(
            MeilisearchModule
                .start(&pm, &Dependency::simple("meilisearch"))
                .is_err()
        );
    }

    #[test]
    fn stop_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            MeilisearchModule
                .stop(&pm, &Dependency::simple("meilisearch"))
                .is_ok()
        );
    }

    #[test]
    fn stop_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            stop_service_fails: true,
            ..Default::default()
        };
        assert!(
            MeilisearchModule
                .stop(&pm, &Dependency::simple("meilisearch"))
                .is_err()
        );
    }
}
