use anyhow::{Context, Result};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep};

pub struct RabbitmqModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "apt" => "rabbitmq-server",
        "winget" => "VMware.RabbitMQ",
        _ => "rabbitmq",
    }
}

fn port(dep: &Dependency) -> u16 {
    dep.extra
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(5672) as u16
}

impl Module for RabbitmqModule {
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
            .with_context(|| format!("RabbitMQ not accepting connections on port {p}"))?;
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
            name: "rabbitmq".into(),
            version: None,
            tap: None,
            after_install: None,
            extra,
        }
    }

    #[test]
    fn rabbitmq_module_is_service() {
        assert!(RabbitmqModule.is_service());
    }

    #[test]
    fn port_defaults_to_5672() {
        let dep = Dependency::simple("rabbitmq");
        assert_eq!(port(&dep), 5672);
    }

    #[test]
    fn port_reads_custom_value() {
        let dep = dep_with_port(5673);
        assert_eq!(port(&dep), 5673);
    }

    #[test]
    fn rabbitmq_health_check_fails_on_unused_port() {
        let dep = dep_with_port(19994);
        let err = RabbitmqModule.health_check(&dep).unwrap_err();
        assert!(err.to_string().contains("19994"));
    }

    #[test]
    fn package_name_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "rabbitmq-server");
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "VMware.RabbitMQ");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "rabbitmq");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            RabbitmqModule
                .is_installed(&pm, &Dependency::simple("rabbitmq"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !RabbitmqModule
                .is_installed(&pm, &Dependency::simple("rabbitmq"))
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
            RabbitmqModule
                .install(&pm, &Dependency::simple("rabbitmq"))
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
            RabbitmqModule
                .is_running(&pm, &Dependency::simple("rabbitmq"))
                .unwrap()
        );
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !RabbitmqModule
                .is_running(&pm, &Dependency::simple("rabbitmq"))
                .unwrap()
        );
    }

    #[test]
    fn start_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            RabbitmqModule
                .start(&pm, &Dependency::simple("rabbitmq"))
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
            RabbitmqModule
                .start(&pm, &Dependency::simple("rabbitmq"))
                .is_err()
        );
    }

    #[test]
    fn stop_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            RabbitmqModule
                .stop(&pm, &Dependency::simple("rabbitmq"))
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
            RabbitmqModule
                .stop(&pm, &Dependency::simple("rabbitmq"))
                .is_err()
        );
    }
}
