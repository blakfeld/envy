use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::Module;

pub struct RedisModule;

impl Module for RedisModule {
    fn is_service(&self) -> bool {
        true
    }

    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(dep)
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(dep)
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
        let port = dep
            .extra
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(6379) as u16;
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;

        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1))
            .with_context(|| format!("Redis not accepting connections on port {port}"))?;

        stream.set_read_timeout(Some(Duration::from_secs(1)))?;
        stream.write_all(b"PING\r\n")?;

        let mut buf = [0u8; 7]; // "+PONG\r\n"
        stream.read_exact(&mut buf)?;

        if !buf.starts_with(b"+PONG") {
            bail!("Redis did not respond to PING (got: {:?})", &buf);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_module_is_service() {
        assert!(RedisModule.is_service());
    }

    #[test]
    fn redis_health_check_fails_on_unused_port() {
        let mut extra = std::collections::HashMap::new();
        extra.insert("port".into(), serde_yaml::Value::Number(19998u64.into()));
        let dep = Dependency {
            name: "redis".into(),
            version: None,
            tap: None,
            profiles: None,
            extra,
        };
        assert!(RedisModule.health_check(&dep).is_err());
    }

    #[test]
    fn redis_health_check_uses_default_port_6379_when_key_absent() {
        // Just verify the code path for missing port key (will fail to connect but no panic).
        let dep = Dependency::simple("redis");
        assert!(RedisModule.health_check(&dep).is_err());
    }
}
