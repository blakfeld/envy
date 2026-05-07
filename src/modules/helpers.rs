use anyhow::{Context, Result};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::time::Duration;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::Module;

/// A simple package install module with per-PM package names.
/// Use for languages and tools that have no special install logic beyond `install_package`.
pub(super) struct PackageModule {
    pub(super) default: &'static str,
    pub(super) apt: &'static str,
    pub(super) winget: &'static str,
}

impl PackageModule {
    pub(super) fn name_for(&self, pm: &dyn PackageManager) -> &'static str {
        match pm.name() {
            "apt" => self.apt,
            "winget" => self.winget,
            _ => self.default,
        }
    }
}

impl Module for PackageModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, self.name_for(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, self.name_for(pm)))
    }
}

/// Writes a `my.cnf` snippet for MySQL-compatible services (MySQL and MariaDB share the same
/// config format). Creates the directory if absent.
pub(super) fn write_mysql_config(
    config_dir: &std::path::Path,
    port: u16,
    cli_args: Option<&str>,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(config_dir)
        .with_context(|| format!("Failed to create config dir {}", config_dir.display()))?;

    let mut ini = format!("[mysqld]\nport = {}\n", port);
    if let Some(args) = cli_args {
        for arg in args.split_whitespace() {
            // Require the -- prefix so bare key=value tokens can't inject directives.
            let Some(rest) = arg.strip_prefix("--") else {
                crate::output::warn(&format!(
                    "mysql cli_args: skipping {:?} — must start with --",
                    arg
                ));
                continue;
            };
            let Some((key, val)) = rest.split_once('=') else {
                crate::output::warn(&format!(
                    "mysql cli_args: skipping {:?} — no = found (expected --key=value)",
                    arg
                ));
                continue;
            };
            // Keys must be safe ini identifiers: alphanumeric, hyphens, underscores.
            if !key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                crate::output::warn(&format!(
                    "mysql cli_args: skipping {:?} — key contains unsafe characters",
                    arg
                ));
                continue;
            }
            // Values must not contain newlines, carriage returns, or null bytes (would break ini structure).
            if val.contains('\n') || val.contains('\r') || val.contains('\0') {
                crate::output::warn(&format!(
                    "mysql cli_args: skipping {:?} — value contains unsafe characters",
                    arg
                ));
                continue;
            }
            ini.push_str(&format!("{} = {}\n", key, val));
        }
    }

    std::fs::write(config_dir.join("my.cnf"), ini).context("Failed to write my.cnf")?;
    Ok(())
}

/// Runs a command, inheriting stdio, and bails on non-zero exit.
pub(super) fn run_cmd(prog: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(prog)
        .args(args)
        .status()
        .with_context(|| format!("failed to start `{prog}`"))?;
    if !status.success() {
        anyhow::bail!("`{prog} {}` failed", args.join(" "));
    }
    Ok(())
}

/// Reads a port value from `dep.extra[key]`, returning an error on overflow or zero.
pub(super) fn extra_port(dep: &Dependency, key: &str, default: u16) -> Result<u16> {
    let raw = dep
        .extra
        .get(key)
        .and_then(|v| v.as_u64())
        .unwrap_or(default as u64);
    if raw == 0 {
        anyhow::bail!("{} value 0 is out of range (must be 1–65535)", key);
    }
    u16::try_from(raw)
        .with_context(|| format!("{} value {} is out of range (must be 1–65535)", key, raw))
}

/// Reads a YAML sequence of strings from dep.extra.
pub(super) fn extra_strs(dep: &Dependency, key: &str) -> Vec<String> {
    dep.extra
        .get(key)
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Returns a copy of `dep` with the name replaced by the platform-appropriate package name.
pub(super) fn pm_dep(dep: &Dependency, name: &str) -> Dependency {
    Dependency {
        name: name.to_string(),
        version: dep.version.clone(),
        tap: dep.tap.clone(),
        after_install: None,
        shell: None,
        extra: HashMap::new(),
    }
}

/// Returns the mtime of `path` as seconds since the Unix epoch, or `None` on failure.
pub(super) fn mtime_secs(path: &std::path::Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// Returns `true` if the stamp at `stamp_path` records the same mtime as `manifest`.
pub(super) fn stamp_matches(stamp_path: &std::path::Path, manifest: &std::path::Path) -> bool {
    let Some(current) = mtime_secs(manifest) else {
        return false;
    };
    std::fs::read_to_string(stamp_path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .is_some_and(|stamped| stamped == current)
}

/// Writes the current mtime of `manifest` to `stamp_path`.
pub(super) fn write_stamp(stamp_path: &std::path::Path, manifest: &std::path::Path) {
    if let Some(secs) = mtime_secs(manifest)
        && let Err(e) = std::fs::write(stamp_path, secs.to_string())
    {
        crate::output::warn(&format!(
            "Could not write stamp file {}: {e} — dependencies will re-run on next `devy up`",
            stamp_path.display()
        ));
    }
}

/// Checks that a service is accepting TCP connections on localhost.
pub(super) fn tcp_ping(port: u16, service: &str) -> Result<()> {
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .with_context(|| format!("{service} not accepting connections on port {port}"))?;
    Ok(())
}

/// Returns the platform-appropriate package name for Node.js.
/// Shared between NodeModule and TypeScriptModule.
pub(super) fn node_pkg(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "apt" => "nodejs",
        "winget" => "OpenJS.NodeJS",
        _ => "node",
    }
}
