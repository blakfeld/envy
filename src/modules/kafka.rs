use anyhow::{Context, Result};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use crate::output;

use super::{Module, pm_dep};

pub struct KafkaModule;

// Kafka is not in standard Ubuntu/Debian apt repos. Users must add the Confluent
// or Apache apt repository manually before `envy up` will succeed on Ubuntu.
// On Homebrew, ZooKeeper is pulled in automatically as a formula dependency.
fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "Apache.Kafka",
        _ => "kafka",
    }
}

fn port(dep: &Dependency) -> u16 {
    dep.extra
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(9092) as u16
}

fn kraft_mode(dep: &Dependency) -> bool {
    dep.extra
        .get("kraft")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

impl Module for KafkaModule {
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
        // ZooKeeper must be running before Kafka in classic mode.
        // In KRaft mode (`kraft: true`) ZooKeeper is not used; we skip it.
        // We also skip it if the start call fails — the user may have set up
        // ZooKeeper through another means or may be running a KRaft build.
        if !kraft_mode(dep)
            && let Err(e) = pm.start_service("zookeeper")
        {
            output::warn(&format!(
                "ZooKeeper failed to start: {e} — Kafka may not start"
            ));
        }
        pm.start_service(&self.service_name(dep))
    }

    fn stop(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.stop_service(&self.service_name(dep))?;
        if !kraft_mode(dep) {
            let _ = pm.stop_service("zookeeper");
        }
        Ok(())
    }

    fn health_check(&self, dep: &Dependency) -> Result<()> {
        let p = port(dep);
        let addr: SocketAddr = format!("127.0.0.1:{p}").parse()?;
        TcpStream::connect_timeout(&addr, Duration::from_secs(1))
            .with_context(|| format!("Kafka not accepting connections on port {p}"))?;
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
        Dependency::with_extra("kafka", extra)
    }

    fn dep_with_kraft(enabled: bool) -> Dependency {
        let mut extra = HashMap::new();
        extra.insert("kraft".into(), serde_yaml::Value::Bool(enabled));
        Dependency::with_extra("kafka", extra)
    }

    #[test]
    fn kafka_module_is_service() {
        assert!(KafkaModule.is_service());
    }

    #[test]
    fn port_defaults_to_9092() {
        let dep = Dependency::simple("kafka");
        assert_eq!(port(&dep), 9092);
    }

    #[test]
    fn port_reads_custom_value() {
        let dep = dep_with_port(9093);
        assert_eq!(port(&dep), 9093);
    }

    #[test]
    fn kraft_mode_defaults_to_false() {
        let dep = Dependency::simple("kafka");
        assert!(!kraft_mode(&dep));
    }

    #[test]
    fn kraft_mode_reads_true() {
        let dep = dep_with_kraft(true);
        assert!(kraft_mode(&dep));
    }

    #[test]
    fn kafka_health_check_fails_on_unused_port() {
        let dep = dep_with_port(19993);
        let err = KafkaModule.health_check(&dep).unwrap_err();
        assert!(err.to_string().contains("19993"));
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "Apache.Kafka");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "kafka");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            KafkaModule
                .is_installed(&pm, &Dependency::simple("kafka"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !KafkaModule
                .is_installed(&pm, &Dependency::simple("kafka"))
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
            KafkaModule
                .install(&pm, &Dependency::simple("kafka"))
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
            KafkaModule
                .is_running(&pm, &Dependency::simple("kafka"))
                .unwrap()
        );
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !KafkaModule
                .is_running(&pm, &Dependency::simple("kafka"))
                .unwrap()
        );
    }

    #[test]
    fn start_in_kraft_mode_skips_zookeeper() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = dep_with_kraft(true);
        assert!(KafkaModule.start(&pm, &dep).is_ok());
        let started = pm.started_services.borrow();
        assert!(
            !started.iter().any(|s| s == "zookeeper"),
            "zookeeper must not be started in kraft mode"
        );
    }

    #[test]
    fn start_in_classic_mode_attempts_zookeeper() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("kafka");
        assert!(KafkaModule.start(&pm, &dep).is_ok());
        let started = pm.started_services.borrow();
        assert!(
            started.iter().any(|s| s == "zookeeper"),
            "zookeeper must be started in classic mode"
        );
    }

    #[test]
    fn start_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            start_service_fails: true,
            ..Default::default()
        };
        let dep = dep_with_kraft(true);
        assert!(KafkaModule.start(&pm, &dep).is_err());
    }

    #[test]
    fn stop_in_kraft_mode_skips_zookeeper() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = dep_with_kraft(true);
        assert!(KafkaModule.stop(&pm, &dep).is_ok());
        let stopped = pm.stopped_services.borrow();
        assert!(
            !stopped.iter().any(|s| s == "zookeeper"),
            "zookeeper must not be stopped in kraft mode"
        );
    }

    #[test]
    fn stop_in_classic_mode_stops_zookeeper() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("kafka");
        assert!(KafkaModule.stop(&pm, &dep).is_ok());
        let stopped = pm.stopped_services.borrow();
        assert!(
            stopped.iter().any(|s| s == "zookeeper"),
            "zookeeper must be stopped in classic mode"
        );
    }

    #[test]
    fn stop_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            stop_service_fails: true,
            ..Default::default()
        };
        let dep = dep_with_kraft(true);
        assert!(KafkaModule.stop(&pm, &dep).is_err());
    }
}
