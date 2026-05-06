use anyhow::{Context, Result};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep};

pub struct MemcachedModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "Memcached.Memcached",
        _ => "memcached",
    }
}

fn port(dep: &Dependency) -> u16 {
    dep.extra
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(11211) as u16
}

impl Module for MemcachedModule {
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
        let addr: SocketAddr = format!("127.0.0.1:{p}").parse()?;
        TcpStream::connect_timeout(&addr, Duration::from_secs(1))
            .with_context(|| format!("Memcached not accepting connections on port {p}"))?;
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
        Dependency {
            name: "memcached".into(),
            version: None,
            tap: None,
            after_install: None,
            extra,
        }
    }

    #[test]
    fn memcached_module_is_service() {
        assert!(MemcachedModule.is_service());
    }

    #[test]
    fn port_defaults_to_11211() {
        let dep = Dependency::simple("memcached");
        assert_eq!(port(&dep), 11211);
    }

    #[test]
    fn port_reads_custom_value() {
        let dep = dep_with_port(11212);
        assert_eq!(port(&dep), 11212);
    }

    #[test]
    fn memcached_health_check_fails_on_unused_port() {
        let dep = dep_with_port(19995);
        let err = MemcachedModule.health_check(&dep).unwrap_err();
        assert!(err.to_string().contains("19995"));
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "Memcached.Memcached");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "memcached");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            MemcachedModule
                .is_installed(&pm, &Dependency::simple("memcached"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !MemcachedModule
                .is_installed(&pm, &Dependency::simple("memcached"))
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
            MemcachedModule
                .install(&pm, &Dependency::simple("memcached"))
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
            MemcachedModule
                .is_running(&pm, &Dependency::simple("memcached"))
                .unwrap()
        );
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !MemcachedModule
                .is_running(&pm, &Dependency::simple("memcached"))
                .unwrap()
        );
    }

    #[test]
    fn start_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            MemcachedModule
                .start(&pm, &Dependency::simple("memcached"))
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
            MemcachedModule
                .start(&pm, &Dependency::simple("memcached"))
                .is_err()
        );
    }

    #[test]
    fn stop_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            MemcachedModule
                .stop(&pm, &Dependency::simple("memcached"))
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
            MemcachedModule
                .stop(&pm, &Dependency::simple("memcached"))
                .is_err()
        );
    }
}
