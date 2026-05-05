use anyhow::{Context, Result};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep};

pub struct MailhogModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "MailHog.MailHog",
        _ => "mailhog",
    }
}

fn smtp_port(dep: &Dependency) -> u16 {
    dep.extra
        .get("smtp_port")
        .and_then(|v| v.as_u64())
        .unwrap_or(1025) as u16
}

impl Module for MailhogModule {
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
        let p = smtp_port(dep);
        let addr: SocketAddr = format!("127.0.0.1:{p}").parse()?;
        TcpStream::connect_timeout(&addr, Duration::from_secs(1))
            .with_context(|| format!("MailHog SMTP not accepting connections on port {p}"))?;
        Ok(())
    }

    fn env_vars(&self, dep: &Dependency) -> HashMap<String, String> {
        let p = smtp_port(dep);
        let mut vars = HashMap::new();
        vars.insert("SMTP_HOST".into(), "127.0.0.1".into());
        vars.insert("SMTP_PORT".into(), p.to_string());
        vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn dep_with_smtp_port(port: u64) -> Dependency {
        let mut extra = HashMap::new();
        extra.insert("smtp_port".into(), serde_yaml::Value::Number(port.into()));
        Dependency::with_extra("mailhog", extra)
    }

    #[test]
    fn mailhog_module_is_service() {
        assert!(MailhogModule.is_service());
    }

    #[test]
    fn smtp_port_defaults_to_1025() {
        let dep = Dependency::simple("mailhog");
        assert_eq!(smtp_port(&dep), 1025);
    }

    #[test]
    fn smtp_port_reads_custom_value() {
        let dep = dep_with_smtp_port(1026);
        assert_eq!(smtp_port(&dep), 1026);
    }

    #[test]
    fn mailhog_health_check_fails_on_unused_port() {
        let dep = dep_with_smtp_port(19988);
        let err = MailhogModule.health_check(&dep).unwrap_err();
        assert!(err.to_string().contains("19988"));
    }

    #[test]
    fn env_vars_includes_smtp_host_and_default_port() {
        let dep = Dependency::simple("mailhog");
        let vars = MailhogModule.env_vars(&dep);
        assert_eq!(vars.get("SMTP_HOST").map(|s| s.as_str()), Some("127.0.0.1"));
        assert_eq!(vars.get("SMTP_PORT").map(|s| s.as_str()), Some("1025"));
    }

    #[test]
    fn env_vars_reflects_custom_smtp_port() {
        let dep = dep_with_smtp_port(2525);
        let vars = MailhogModule.env_vars(&dep);
        assert_eq!(vars.get("SMTP_PORT").map(|s| s.as_str()), Some("2525"));
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "MailHog.MailHog");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "mailhog");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            MailhogModule
                .is_installed(&pm, &Dependency::simple("mailhog"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !MailhogModule
                .is_installed(&pm, &Dependency::simple("mailhog"))
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
            MailhogModule
                .install(&pm, &Dependency::simple("mailhog"))
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
            MailhogModule
                .is_running(&pm, &Dependency::simple("mailhog"))
                .unwrap()
        );
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !MailhogModule
                .is_running(&pm, &Dependency::simple("mailhog"))
                .unwrap()
        );
    }

    #[test]
    fn start_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            MailhogModule
                .start(&pm, &Dependency::simple("mailhog"))
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
            MailhogModule
                .start(&pm, &Dependency::simple("mailhog"))
                .is_err()
        );
    }

    #[test]
    fn stop_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            MailhogModule
                .stop(&pm, &Dependency::simple("mailhog"))
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
            MailhogModule
                .stop(&pm, &Dependency::simple("mailhog"))
                .is_err()
        );
    }
}
