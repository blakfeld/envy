use anyhow::{Context, Result};
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use crate::output;

use super::{Module, pm_dep};

pub struct PostgresModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "PostgreSQL.PostgreSQL",
        _ => "postgresql",
    }
}

fn port(dep: &Dependency) -> u16 {
    dep.extra
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(5432) as u16
}

fn write_config(config_dir: &Path, port: u16) -> Result<()> {
    fs::create_dir_all(config_dir).context("Failed to create postgresql config dir")?;
    let conf = format!("# envy-managed\nport = {port}\n");
    fs::write(config_dir.join("envy.conf"), conf)
        .context("Failed to write postgresql envy.conf")?;
    Ok(())
}

impl Module for PostgresModule {
    fn is_service(&self) -> bool {
        true
    }

    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, package_name(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, package_name(pm)))?;

        let p = port(dep);
        if p != 5432 {
            match pm.service_config_dir("postgresql") {
                Some(config_dir) => write_config(&config_dir, p)?,
                None => { output::warn(&format!(
                    "port ignored: {} does not support service config dirs",
                    pm.name()
                )); }
            }
        }

        Ok(())
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
        let addr: SocketAddr = format!("127.0.0.1:{p}").parse()?;
        TcpStream::connect_timeout(&addr, Duration::from_secs(1))
            .with_context(|| format!("PostgreSQL not accepting connections on port {p}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn dep_with_port(port: u64) -> Dependency {
        let mut extra = HashMap::new();
        extra.insert("port".into(), serde_yaml::Value::Number(port.into()));
        Dependency::with_extra("postgresql", extra)
    }

    #[test]
    fn postgres_module_is_service() {
        assert!(PostgresModule.is_service());
    }

    #[test]
    fn port_defaults_to_5432() {
        let dep = Dependency::simple("postgresql");
        assert_eq!(port(&dep), 5432);
    }

    #[test]
    fn port_reads_custom_value() {
        let dep = dep_with_port(5433);
        assert_eq!(port(&dep), 5433);
    }

    #[test]
    fn postgres_health_check_fails_on_unused_port() {
        let dep = dep_with_port(19997);
        let err = PostgresModule.health_check(&dep).unwrap_err();
        assert!(err.to_string().contains("19997"));
    }

    #[test]
    fn write_config_creates_conf_file() {
        let dir = std::env::temp_dir().join(format!(
            "envy_pg_test_{}",
            std::process::id()
        ));
        write_config(&dir, 5433).unwrap();
        let content = std::fs::read_to_string(dir.join("envy.conf")).unwrap();
        assert!(content.contains("port = 5433"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_name_apt() {
        let pm = crate::package_manager::MockPackageManager { name: "apt", ..Default::default() };
        assert_eq!(package_name(&pm), "postgresql");
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager { name: "winget", ..Default::default() };
        assert_eq!(package_name(&pm), "PostgreSQL.PostgreSQL");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager { name: "brew", ..Default::default() };
        assert_eq!(package_name(&pm), "postgresql");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager { installed: true, ..Default::default() };
        assert!(PostgresModule.is_installed(&pm, &Dependency::simple("postgresql")).unwrap());
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(!PostgresModule.is_installed(&pm, &Dependency::simple("postgresql")).unwrap());
    }

    #[test]
    fn install_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager { install_fails: true, ..Default::default() };
        assert!(PostgresModule.install(&pm, &Dependency::simple("postgresql")).is_err());
    }

    #[test]
    fn install_writes_config_for_non_default_port() {
        let dir = std::env::temp_dir().join(format!("envy_pg_install_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pm = crate::package_manager::MockPackageManager {
            config_dir: Some(dir.clone()),
            ..Default::default()
        };
        let dep = dep_with_port(5433);
        PostgresModule.install(&pm, &dep).unwrap();
        let content = std::fs::read_to_string(dir.join("envy.conf")).unwrap();
        assert!(content.contains("port = 5433"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_skips_config_for_default_port() {
        let dir = std::env::temp_dir().join(format!("envy_pg_skip_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pm = crate::package_manager::MockPackageManager {
            config_dir: Some(dir.clone()),
            ..Default::default()
        };
        let dep = Dependency::simple("postgresql");
        PostgresModule.install(&pm, &dep).unwrap();
        assert!(!dir.join("envy.conf").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_running_true() {
        let pm = crate::package_manager::MockPackageManager { service_running: true, ..Default::default() };
        assert!(PostgresModule.is_running(&pm, &Dependency::simple("postgresql")).unwrap());
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(!PostgresModule.is_running(&pm, &Dependency::simple("postgresql")).unwrap());
    }

    #[test]
    fn start_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(PostgresModule.start(&pm, &Dependency::simple("postgresql")).is_ok());
    }

    #[test]
    fn start_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager { start_service_fails: true, ..Default::default() };
        assert!(PostgresModule.start(&pm, &Dependency::simple("postgresql")).is_err());
    }

    #[test]
    fn stop_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(PostgresModule.stop(&pm, &Dependency::simple("postgresql")).is_ok());
    }

    #[test]
    fn stop_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager { stop_service_fails: true, ..Default::default() };
        assert!(PostgresModule.stop(&pm, &Dependency::simple("postgresql")).is_err());
    }
}
