use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use crate::output;

use super::{Module, pm_dep};

pub struct MinioModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "MinIO.MinIO",
        _ => "minio",
    }
}

fn port(dep: &Dependency) -> u16 {
    dep.extra
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(9000) as u16
}

impl Module for MinioModule {
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
        let url = format!("http://127.0.0.1:{p}/minio/health/live");
        ureq::get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .call()
            .with_context(|| format!("MinIO not reachable on port {p}"))?;
        Ok(())
    }

    fn env_vars(&self, dep: &Dependency) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        if let Some(key) = dep.extra.get("access_key").and_then(|v| v.as_str()) {
            vars.insert("MINIO_ROOT_USER".into(), key.to_string());
        }
        if let Some(secret) = dep.extra.get("secret_key").and_then(|v| v.as_str()) {
            vars.insert("MINIO_ROOT_PASSWORD".into(), secret.to_string());
        }
        if !vars.is_empty() {
            output::warn("minio credentials written to plaintext shadowenv — consider using ejson secrets instead");
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
        Dependency::with_extra("minio", extra)
    }

    #[test]
    fn minio_module_is_service() {
        assert!(MinioModule.is_service());
    }

    #[test]
    fn port_defaults_to_9000() {
        let dep = Dependency::simple("minio");
        assert_eq!(port(&dep), 9000);
    }

    #[test]
    fn port_reads_custom_value() {
        let dep = dep_with_port(9001);
        assert_eq!(port(&dep), 9001);
    }

    #[test]
    fn minio_health_check_fails_on_unused_port() {
        let dep = dep_with_port(19989);
        let err = MinioModule.health_check(&dep).unwrap_err();
        assert!(err.to_string().contains("19989"));
    }

    #[test]
    fn env_vars_empty_when_no_keys_configured() {
        let dep = Dependency::simple("minio");
        assert!(MinioModule.env_vars(&dep).is_empty());
    }

    #[test]
    fn env_vars_includes_credentials_when_configured() {
        let mut extra = HashMap::new();
        extra.insert(
            "access_key".into(),
            serde_yaml::Value::String("myuser".into()),
        );
        extra.insert(
            "secret_key".into(),
            serde_yaml::Value::String("mypassword".into()),
        );
        let dep = Dependency::with_extra("minio", extra);
        let vars = MinioModule.env_vars(&dep);
        assert_eq!(vars.get("MINIO_ROOT_USER").map(|s| s.as_str()), Some("myuser"));
        assert_eq!(vars.get("MINIO_ROOT_PASSWORD").map(|s| s.as_str()), Some("mypassword"));
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager { name: "winget", ..Default::default() };
        assert_eq!(package_name(&pm), "MinIO.MinIO");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager { name: "brew", ..Default::default() };
        assert_eq!(package_name(&pm), "minio");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager { installed: true, ..Default::default() };
        assert!(MinioModule.is_installed(&pm, &Dependency::simple("minio")).unwrap());
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(!MinioModule.is_installed(&pm, &Dependency::simple("minio")).unwrap());
    }

    #[test]
    fn install_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager { install_fails: true, ..Default::default() };
        assert!(MinioModule.install(&pm, &Dependency::simple("minio")).is_err());
    }

    #[test]
    fn is_running_true() {
        let pm = crate::package_manager::MockPackageManager { service_running: true, ..Default::default() };
        assert!(MinioModule.is_running(&pm, &Dependency::simple("minio")).unwrap());
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(!MinioModule.is_running(&pm, &Dependency::simple("minio")).unwrap());
    }

    #[test]
    fn start_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(MinioModule.start(&pm, &Dependency::simple("minio")).is_ok());
    }

    #[test]
    fn start_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager { start_service_fails: true, ..Default::default() };
        assert!(MinioModule.start(&pm, &Dependency::simple("minio")).is_err());
    }

    #[test]
    fn stop_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(MinioModule.stop(&pm, &Dependency::simple("minio")).is_ok());
    }

    #[test]
    fn stop_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager { stop_service_fails: true, ..Default::default() };
        assert!(MinioModule.stop(&pm, &Dependency::simple("minio")).is_err());
    }

    #[test]
    fn env_vars_empty_returns_empty_not_non_empty() {
        let dep = Dependency::simple("minio");
        let vars = MinioModule.env_vars(&dep);
        assert!(vars.is_empty());
    }

    #[test]
    fn env_vars_non_empty_returns_non_empty() {
        let mut extra = HashMap::new();
        extra.insert("access_key".into(), serde_yaml::Value::String("user".into()));
        extra.insert("secret_key".into(), serde_yaml::Value::String("pass".into()));
        let dep = Dependency::with_extra("minio", extra);
        let vars = MinioModule.env_vars(&dep);
        assert!(!vars.is_empty(), "Expected non-empty vars when credentials are configured");
    }

    #[test]
    fn env_vars_warns_only_when_credentials_present() {
        use crate::output::WARN_CALL_COUNT;
        use std::sync::atomic::Ordering;

        // Verify warn is called when credentials are present but not when absent.
        let before_with = WARN_CALL_COUNT.load(Ordering::Relaxed);
        let mut extra = HashMap::new();
        extra.insert("access_key".into(), serde_yaml::Value::String("u".into()));
        extra.insert("secret_key".into(), serde_yaml::Value::String("p".into()));
        let dep_with = Dependency::with_extra("minio", extra);
        let _ = MinioModule.env_vars(&dep_with);
        let after_with = WARN_CALL_COUNT.load(Ordering::Relaxed);
        assert!(after_with > before_with, "warn must be called when credentials are configured");

        // Without credentials: warn should NOT be called.
        let before_without = WARN_CALL_COUNT.load(Ordering::Relaxed);
        let dep_without = Dependency::simple("minio");
        let _ = MinioModule.env_vars(&dep_without);
        let after_without = WARN_CALL_COUNT.load(Ordering::Relaxed);
        assert_eq!(before_without, after_without, "warn must not be called for empty credentials");
    }
}
