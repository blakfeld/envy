use anyhow::{Context, Result};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::config::Dependency;
#[cfg(target_os = "linux")]
use crate::output;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep};

pub struct NginxModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "Nginx.Nginx",
        "nix" => "nginx",
        _ => "nginx",
    }
}

fn port(dep: &Dependency) -> anyhow::Result<u16> {
    super::extra_port(dep, "port", 80)
}

impl Module for NginxModule {
    fn is_service(&self) -> bool {
        true
    }

    fn service_exec_name(&self) -> Option<&'static str> {
        Some("nginx")
    }

    fn nix_attr(&self, _dep: &crate::config::Dependency) -> Option<String> {
        Some("nginx".to_string())
    }

    fn default_port(&self) -> Option<u16> {
        Some(80)
    }
    fn known_extra_keys(&self) -> Option<&'static [&'static str]> {
        Some(&["port"])
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
        #[cfg(target_os = "linux")]
        {
            let p = port(dep)?;
            if p < 1024 {
                output::warn(&format!(
                    "nginx: port {p} requires root or CAP_NET_BIND_SERVICE on Linux. \
                     Set a port >= 1024 in devy.yml if this fails."
                ));
            }
        }
        pm.start_service(&self.service_name(dep))
    }

    fn stop(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.stop_service(&self.service_name(dep))
    }

    fn health_check(&self, dep: &Dependency) -> Result<()> {
        let p = port(dep)?;
        let addr: SocketAddr = format!("127.0.0.1:{p}").parse()?;
        TcpStream::connect_timeout(&addr, Duration::from_secs(1))
            .with_context(|| format!("nginx not accepting connections on port {p}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nginx_module_is_service() {
        assert!(NginxModule.is_service());
    }

    #[test]
    fn port_defaults_to_80() {
        let dep = Dependency::simple("nginx");
        assert_eq!(port(&dep).unwrap(), 80);
    }

    #[test]
    fn port_bails_on_out_of_range() {
        let mut extra = std::collections::HashMap::new();
        extra.insert(
            "port".into(),
            crate::config::ExtraValue::Number(99999u64.into()),
        );
        let dep = Dependency::with_extra("nginx", extra);
        assert!(port(&dep).is_err());
    }

    #[test]
    fn nginx_health_check_fails_on_unused_port() {
        let mut extra = std::collections::HashMap::new();
        extra.insert(
            "port".into(),
            crate::config::ExtraValue::Number(19995u64.into()),
        );
        let dep = Dependency::with_extra("nginx", extra);
        assert!(NginxModule.health_check(&dep).is_err());
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "Nginx.Nginx");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "nginx");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            NginxModule
                .is_installed(&pm, &Dependency::simple("nginx"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !NginxModule
                .is_installed(&pm, &Dependency::simple("nginx"))
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
            NginxModule
                .install(&pm, &Dependency::simple("nginx"))
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
            NginxModule
                .is_running(&pm, &Dependency::simple("nginx"))
                .unwrap()
        );
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !NginxModule
                .is_running(&pm, &Dependency::simple("nginx"))
                .unwrap()
        );
    }

    #[test]
    fn start_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(NginxModule.start(&pm, &Dependency::simple("nginx")).is_ok());
    }

    #[test]
    fn start_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            start_service_fails: true,
            ..Default::default()
        };
        assert!(
            NginxModule
                .start(&pm, &Dependency::simple("nginx"))
                .is_err()
        );
    }

    #[test]
    fn stop_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(NginxModule.stop(&pm, &Dependency::simple("nginx")).is_ok());
    }

    #[test]
    fn stop_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            stop_service_fails: true,
            ..Default::default()
        };
        assert!(NginxModule.stop(&pm, &Dependency::simple("nginx")).is_err());
    }
}
