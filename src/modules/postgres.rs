use anyhow::{Context, Result};
use std::borrow::Cow;
use std::fs;
use std::io::{Read, Write};
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

fn port(dep: &Dependency) -> anyhow::Result<u16> {
    super::extra_port(dep, "port", 5432)
}

fn write_config(config_dir: &Path, port: u16) -> Result<()> {
    fs::create_dir_all(config_dir).context("Failed to create postgresql config dir")?;
    let conf = format!("# devy-managed\nport = {port}\n");
    fs::write(config_dir.join("devy.conf"), conf)
        .context("Failed to write postgresql devy.conf")?;
    Ok(())
}

impl Module for PostgresModule {
    fn is_service(&self) -> bool {
        true
    }
    fn default_port(&self) -> Option<u16> {
        Some(5432)
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

    fn post_setup(
        &self,
        dep: &Dependency,
        pm: &dyn PackageManager,
        _project_root: &std::path::Path,
    ) -> Result<()> {
        let p = port(dep)?;
        if p != 5432 {
            match pm.service_config_dir("postgresql") {
                Some(config_dir) => write_config(&config_dir, p)?,
                None => {
                    output::warn(&format!(
                        "port ignored: {} does not support service config dirs",
                        pm.name()
                    ));
                }
            }
        }
        Ok(())
    }

    fn service_name<'a>(&self, _dep: &'a Dependency) -> Cow<'a, str> {
        Cow::Borrowed("postgresql")
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
        let p = port(dep)?;
        let addr: SocketAddr = format!("127.0.0.1:{p}").parse()?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1))
            .with_context(|| format!("PostgreSQL not accepting connections on port {p}"))?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        // Send a minimal StartupMessage to trigger an auth response ('R') or error ('E').
        // Message format: length (4 bytes BE) + protocol version 3.0 (4 bytes BE).
        let msg: &[u8] = &[0x00, 0x00, 0x00, 0x08, 0x00, 0x03, 0x00, 0x00];
        stream.write_all(msg)?;
        let mut first = [0u8; 1];
        stream.read_exact(&mut first)?;
        anyhow::ensure!(
            first[0] == b'R' || first[0] == b'E',
            "PostgreSQL on port {p} returned unexpected startup response byte: 0x{:02x}",
            first[0]
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn dep_with_port(port: u64) -> Dependency {
        let mut extra = HashMap::new();
        extra.insert(
            "port".into(),
            crate::config::ExtraValue::Number(port.into()),
        );
        Dependency::with_extra("postgresql", extra)
    }

    #[test]
    fn postgres_module_is_service() {
        assert!(PostgresModule.is_service());
    }

    #[test]
    fn service_name_is_always_postgresql() {
        let dep = Dependency::simple("postgresql");
        assert_eq!(PostgresModule.service_name(&dep).as_ref(), "postgresql");
        let dep2 = Dependency::simple("postgres");
        assert_eq!(PostgresModule.service_name(&dep2).as_ref(), "postgresql");
    }

    #[test]
    fn port_defaults_to_5432() {
        let dep = Dependency::simple("postgresql");
        assert_eq!(port(&dep).unwrap(), 5432);
    }

    #[test]
    fn port_reads_custom_value() {
        let dep = dep_with_port(5433);
        assert_eq!(port(&dep).unwrap(), 5433);
    }

    #[test]
    fn port_bails_on_out_of_range() {
        let dep = dep_with_port(99999);
        assert!(port(&dep).is_err());
    }

    #[test]
    fn postgres_health_check_fails_on_unused_port() {
        let dep = dep_with_port(19997);
        let err = PostgresModule.health_check(&dep).unwrap_err();
        assert!(err.to_string().contains("19997"));
    }

    #[test]
    fn write_config_creates_conf_file() {
        let dir = crate::test_support::tmp_dir();
        write_config(&dir, 5433).unwrap();
        let content = std::fs::read_to_string(dir.join("devy.conf")).unwrap();
        assert!(content.contains("port = 5433"));
    }

    #[test]
    fn package_name_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "postgresql");
    }

    #[test]
    fn package_name_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "PostgreSQL.PostgreSQL");
    }

    #[test]
    fn package_name_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(package_name(&pm), "postgresql");
    }

    #[test]
    fn is_installed_true() {
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            PostgresModule
                .is_installed(&pm, &Dependency::simple("postgresql"))
                .unwrap()
        );
    }

    #[test]
    fn is_installed_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !PostgresModule
                .is_installed(&pm, &Dependency::simple("postgresql"))
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
            PostgresModule
                .install(&pm, &Dependency::simple("postgresql"))
                .is_err()
        );
    }

    #[test]
    fn post_setup_writes_config_for_non_default_port() {
        let dir = crate::test_support::tmp_dir();
        let pm = crate::package_manager::MockPackageManager {
            config_dir: Some(dir.to_path_buf()),
            ..Default::default()
        };
        let dep = dep_with_port(5433);
        PostgresModule
            .post_setup(&dep, &pm, std::path::Path::new("/tmp"))
            .unwrap();
        let content = std::fs::read_to_string(dir.join("devy.conf")).unwrap();
        assert!(content.contains("port = 5433"));
    }

    #[test]
    fn post_setup_skips_config_for_default_port() {
        let dir = crate::test_support::tmp_dir();
        let pm = crate::package_manager::MockPackageManager {
            config_dir: Some(dir.to_path_buf()),
            ..Default::default()
        };
        let dep = Dependency::simple("postgresql");
        PostgresModule
            .post_setup(&dep, &pm, std::path::Path::new("/tmp"))
            .unwrap();
        assert!(!dir.join("devy.conf").exists());
    }

    #[test]
    fn is_running_true() {
        let pm = crate::package_manager::MockPackageManager {
            service_running: true,
            ..Default::default()
        };
        assert!(
            PostgresModule
                .is_running(&pm, &Dependency::simple("postgresql"))
                .unwrap()
        );
    }

    #[test]
    fn is_running_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            !PostgresModule
                .is_running(&pm, &Dependency::simple("postgresql"))
                .unwrap()
        );
    }

    #[test]
    fn start_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            PostgresModule
                .start(&pm, &Dependency::simple("postgresql"))
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
            PostgresModule
                .start(&pm, &Dependency::simple("postgresql"))
                .is_err()
        );
    }

    #[test]
    fn stop_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        assert!(
            PostgresModule
                .stop(&pm, &Dependency::simple("postgresql"))
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
            PostgresModule
                .stop(&pm, &Dependency::simple("postgresql"))
                .is_err()
        );
    }
}
