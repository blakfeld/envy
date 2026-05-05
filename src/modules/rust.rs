use anyhow::{Context, Result, bail};
use std::process::{Command, Stdio};
use which::which;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::{Module, extra_strs, run_cmd};

pub struct RustModule;

impl RustModule {
    fn toolchain(dep: &Dependency) -> &str {
        dep.extra
            .get("toolchain")
            .and_then(|v| v.as_str())
            .unwrap_or("stable")
    }
}

/// ~/.cargo/bin/rustup, used when rustup is not yet on PATH after a fresh install.
fn rustup_bin() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/.cargo/bin/rustup")
}

impl Module for RustModule {
    fn is_installed(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<bool> {
        Ok(which("rustup").is_ok() || which("cargo").is_ok())
    }

    fn install(&self, _pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        if which("rustup").is_err() {
            let status = Command::new("sh")
                .arg("-c")
                .arg(concat!(
                    "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs",
                    " | sh -s -- -y --no-modify-path"
                ))
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run rustup installer")?;
            if !status.success() {
                bail!("rustup installation failed");
            }
        }

        let ru = rustup_bin();
        let toolchain = Self::toolchain(dep);

        run_cmd(&ru, &["toolchain", "install", toolchain])?;
        run_cmd(&ru, &["default", toolchain])?;

        for target in extra_strs(dep, "targets") {
            run_cmd(&ru, &["target", "add", "--toolchain", toolchain, &target])?;
        }
        for component in extra_strs(dep, "components") {
            run_cmd(
                &ru,
                &["component", "add", "--toolchain", toolchain, &component],
            )?;
        }

        Ok(())
    }

    fn source(&self) -> &'static str {
        "rustup"
    }

    fn resolved_version(
        &self,
        _pm: &dyn PackageManager,
        _dep: &Dependency,
    ) -> Result<Option<String>> {
        let rustc = format!(
            "{}/.cargo/bin/rustc",
            std::env::var("HOME").unwrap_or_default()
        );
        let out = Command::new(rustc).arg("--version").output();
        // "rustc 1.78.0 (9b00956e5 2024-04-29)" — take second token
        Ok(out.ok().and_then(|o| {
            String::from_utf8(o.stdout)
                .ok()?
                .split_whitespace()
                .nth(1)
                .map(String::from)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn dep_with_toolchain(toolchain: &str) -> Dependency {
        let mut extra = HashMap::new();
        extra.insert(
            "toolchain".into(),
            serde_yaml::Value::String(toolchain.into()),
        );
        Dependency {
            name: "rust".into(),
            version: None,
            tap: None,
            profiles: None,
            extra,
        }
    }

    // ── toolchain ─────────────────────────────────────────────────────────────

    #[test]
    fn toolchain_defaults_to_stable() {
        let dep = Dependency::simple("rust");
        assert_eq!(RustModule::toolchain(&dep), "stable");
    }

    #[test]
    fn toolchain_reads_custom_value() {
        let dep = dep_with_toolchain("nightly");
        assert_eq!(RustModule::toolchain(&dep), "nightly");
    }

    #[test]
    fn toolchain_reads_pinned_version() {
        let dep = dep_with_toolchain("1.78.0");
        assert_eq!(RustModule::toolchain(&dep), "1.78.0");
    }

    // ── source ────────────────────────────────────────────────────────────────

    #[test]
    fn rust_module_source_is_rustup() {
        assert_eq!(RustModule.source(), "rustup");
    }
}
