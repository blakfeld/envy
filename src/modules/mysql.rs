use anyhow::{Context, Result};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use crate::output;

use super::{Module, pm_dep, write_mysql_config};

pub struct MysqlModule;

fn package_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "apt" => "mysql-server",
        "winget" => "Oracle.MySQL",
        _ => "mysql",
    }
}

fn port(dep: &Dependency) -> u16 {
    dep.extra
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(3306) as u16
}

fn cli_args(dep: &Dependency) -> Option<String> {
    dep.extra
        .get("cli_args")
        .and_then(|v| v.as_str())
        .map(String::from)
}

impl Module for MysqlModule {
    fn is_service(&self) -> bool {
        true
    }

    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, package_name(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, package_name(pm)))?;

        let p = port(dep);
        let args = cli_args(dep);
        if p != 3306 || args.is_some() {
            match pm.service_config_dir("mysql") {
                Some(config_dir) => write_mysql_config(&config_dir, p, args.as_deref())?,
                None => {
                    output::warn(&format!(
                        "port/cli_args ignored: {} does not support service config dirs",
                        pm.name()
                    ));
                }
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
            .with_context(|| format!("MySQL not accepting connections on port {p}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn dep_with_extra(extra: HashMap<String, serde_yaml::Value>) -> Dependency {
        Dependency {
            name: "mysql".into(),
            version: None,
            tap: None,
            after_install: None,
            extra,
        }
    }

    // ── port ──────────────────────────────────────────────────────────────────

    #[test]
    fn port_defaults_to_3306() {
        let dep = Dependency::simple("mysql");
        assert_eq!(port(&dep), 3306);
    }

    #[test]
    fn port_reads_custom_value() {
        let mut extra = HashMap::new();
        extra.insert("port".into(), serde_yaml::Value::Number(3307.into()));
        let dep = dep_with_extra(extra);
        assert_eq!(port(&dep), 3307);
    }

    #[test]
    fn port_ignores_non_numeric_value() {
        let mut extra = HashMap::new();
        extra.insert("port".into(), serde_yaml::Value::String("bogus".into()));
        let dep = dep_with_extra(extra);
        assert_eq!(port(&dep), 3306);
    }

    // ── cli_args ──────────────────────────────────────────────────────────────

    #[test]
    fn cli_args_absent_returns_none() {
        let dep = Dependency::simple("mysql");
        assert!(cli_args(&dep).is_none());
    }

    #[test]
    fn cli_args_present_returns_string() {
        let mut extra = HashMap::new();
        extra.insert(
            "cli_args".into(),
            serde_yaml::Value::String("--innodb-buffer-pool-size=256M".into()),
        );
        let dep = dep_with_extra(extra);
        assert_eq!(
            cli_args(&dep).as_deref(),
            Some("--innodb-buffer-pool-size=256M")
        );
    }

    // ── MysqlModule trait methods ─────────────────────────────────────────────

    #[test]
    fn mysql_module_is_service() {
        assert!(MysqlModule.is_service());
    }

    #[test]
    fn mysql_health_check_fails_on_unused_port() {
        let mut extra = HashMap::new();
        extra.insert("port".into(), serde_yaml::Value::Number(19999u64.into()));
        let dep = dep_with_extra(extra);
        let err = MysqlModule.health_check(&dep).unwrap_err();
        assert!(err.to_string().contains("19999"));
    }

    #[test]
    fn package_name_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "mysql-server");
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "Oracle.MySQL");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "mysql");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            MysqlModule
                .is_installed(&pm, &Dependency::simple("mysql"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !MysqlModule
                .is_installed(&pm, &Dependency::simple("mysql"))
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
            MysqlModule
                .install(&pm, &Dependency::simple("mysql"))
                .is_err()
        );
    }

    #[test]
    fn install_writes_config_when_custom_port_and_config_dir_available() {
        let dir = std::env::temp_dir().join(format!("devy_mysql_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pm = crate::package_manager::MockPackageManager {
            config_dir: Some(dir.clone()),
            ..Default::default()
        };
        let mut extra = HashMap::new();
        extra.insert("port".into(), serde_yaml::Value::Number(3307u64.into()));
        let dep = dep_with_extra(extra);
        MysqlModule.install(&pm, &dep).unwrap();
        let content = std::fs::read_to_string(dir.join("my.cnf")).unwrap();
        assert!(content.contains("port = 3307"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_writes_config_when_default_port_but_cli_args_set() {
        // Tests the || condition: even with default port, cli_args trigger config write.
        let dir = std::env::temp_dir().join(format!("devy_mysql_test_args_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pm = crate::package_manager::MockPackageManager {
            config_dir: Some(dir.clone()),
            ..Default::default()
        };
        let mut extra = HashMap::new();
        extra.insert(
            "cli_args".into(),
            serde_yaml::Value::String("--innodb-buffer-pool-size=256M".into()),
        );
        let dep = dep_with_extra(extra);
        MysqlModule.install(&pm, &dep).unwrap();
        let content = std::fs::read_to_string(dir.join("my.cnf")).unwrap();
        assert!(content.contains("innodb-buffer-pool-size"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_skips_config_when_default_port_no_args() {
        // With default port and no cli_args, config file should NOT be written.
        let dir = std::env::temp_dir().join(format!("devy_mysql_test_skip_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pm = crate::package_manager::MockPackageManager {
            config_dir: Some(dir.clone()),
            ..Default::default()
        };
        let dep = Dependency::simple("mysql");
        MysqlModule.install(&pm, &dep).unwrap();
        assert!(!dir.join("my.cnf").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_running_true() {
        let pm = crate::package_manager::MockPackageManager {
            service_running: true,
            ..Default::default()
        };
        assert!(
            MysqlModule
                .is_running(&pm, &Dependency::simple("mysql"))
                .unwrap()
        );
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !MysqlModule
                .is_running(&pm, &Dependency::simple("mysql"))
                .unwrap()
        );
    }

    #[test]
    fn start_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(MysqlModule.start(&pm, &Dependency::simple("mysql")).is_ok());
    }

    #[test]
    fn start_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            start_service_fails: true,
            ..Default::default()
        };
        assert!(
            MysqlModule
                .start(&pm, &Dependency::simple("mysql"))
                .is_err()
        );
    }

    #[test]
    fn stop_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(MysqlModule.stop(&pm, &Dependency::simple("mysql")).is_ok());
    }

    #[test]
    fn stop_propagates_pm_error() {
        let pm = crate::package_manager::MockPackageManager {
            stop_service_fails: true,
            ..Default::default()
        };
        assert!(MysqlModule.stop(&pm, &Dependency::simple("mysql")).is_err());
    }
}
