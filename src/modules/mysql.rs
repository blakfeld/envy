use anyhow::{Context, Result};
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::time::Duration;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::Module;

pub struct MysqlModule;

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

fn write_config(port: u16, cli_args: Option<&str>) -> Result<()> {
    let output = Command::new("brew")
        .args(["--prefix", "mysql"])
        .output()
        .context("Failed to resolve mysql brew prefix")?;
    let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let config_dir = std::path::Path::new(&prefix).join("etc");
    fs::create_dir_all(&config_dir).context("Failed to create mysql config dir")?;

    let mut ini = format!("[mysqld]\nport = {}\n", port);
    if let Some(args) = cli_args {
        for arg in args.split_whitespace() {
            let stripped = arg.trim_start_matches('-');
            if let Some((key, val)) = stripped.split_once('=') {
                ini.push_str(&format!("{} = {}\n", key, val));
            }
        }
    }

    fs::write(config_dir.join("my.cnf"), ini).context("Failed to write my.cnf")?;
    Ok(())
}

impl Module for MysqlModule {
    fn is_service(&self) -> bool {
        true
    }

    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(dep)
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(dep)?;

        let p = port(dep);
        let args = cli_args(dep);
        if p != 3306 || args.is_some() {
            write_config(p, args.as_deref())?;
        }

        Ok(())
    }

    fn is_running(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_service_running(&dep.name)
    }

    fn start(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.start_service(&dep.name)
    }

    fn stop(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.stop_service(&dep.name)
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
            profiles: None,
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
}
