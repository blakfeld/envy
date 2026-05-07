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
/// Returns None when HOME is unset or empty (CI/sudo/Docker environments).
fn rustup_bin() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(|home| format!("{home}/.cargo/bin/rustup"))
}

impl Module for RustModule {
    fn known_extra_keys(&self) -> Option<&'static [&'static str]> {
        Some(&["toolchain", "targets", "components"])
    }

    fn is_installed(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<bool> {
        Ok(which("rustup").is_ok())
    }

    fn install(&self, _pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        if which("rustup").is_err() {
            let installer_cmd =
                std::env::var("DEVY_TEST_RUSTUP_INSTALLER_SCRIPT").unwrap_or_else(|_| {
                    concat!(
                        "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs",
                        " | sh -s -- -y --no-modify-path"
                    )
                    .to_string()
                });

            let status = Command::new("sh")
                .arg("-c")
                .arg(&installer_cmd)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .context("Failed to run rustup installer")?;
            if !status.success() {
                bail!("rustup installation failed");
            }
        }

        let ru = rustup_bin().unwrap_or_else(|| "rustup".to_string());
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

    fn source(&self) -> Option<&'static str> {
        Some("rustup")
    }

    fn resolved_version(
        &self,
        _pm: &dyn PackageManager,
        _dep: &Dependency,
    ) -> Result<Option<String>> {
        let rustc = std::env::var("HOME")
            .ok()
            .filter(|h| !h.is_empty())
            .map(|home| format!("{home}/.cargo/bin/rustc"))
            .unwrap_or_else(|| "rustc".to_string());
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
            crate::config::ExtraValue::String(toolchain.into()),
        );
        Dependency::with_extra("rust", extra)
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
        assert_eq!(RustModule.source(), Some("rustup"));
    }

    // ── rustup_bin ────────────────────────────────────────────────────────────

    #[test]
    fn rustup_bin_returns_some_with_non_empty_home() {
        if std::env::var("HOME")
            .map(|h| !h.is_empty())
            .unwrap_or(false)
        {
            let bin = rustup_bin().unwrap();
            assert!(
                bin.contains(".cargo/bin/rustup"),
                "Expected ~/.cargo/bin/rustup, got {bin}"
            );
        }
    }

    #[test]
    fn rustup_bin_path_contains_home() {
        if let Ok(home) = std::env::var("HOME")
            && !home.is_empty()
        {
            let bin = rustup_bin().unwrap();
            assert!(bin.starts_with(&home));
        }
    }

    // ── is_installed ─────────────────────────────────────────────────────────

    #[test]
    fn rust_is_installed_does_not_panic() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("rust");
        let _ = RustModule.is_installed(&pm, &dep);
    }

    #[test]
    fn rust_is_installed_true_when_cargo_on_path() {
        // cargo is always on PATH in a Rust development environment.
        if which::which("cargo").is_err() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("rust");
        assert!(RustModule.is_installed(&pm, &dep).unwrap());
    }

    // ── resolved_version ─────────────────────────────────────────────────────

    #[test]
    fn rust_resolved_version_returns_ok() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("rust");
        assert!(RustModule.resolved_version(&pm, &dep).is_ok());
    }

    #[test]
    fn rust_resolved_version_is_some_when_rustc_installed() {
        if which::which("rustc").is_err() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("rust");
        let ver = RustModule.resolved_version(&pm, &dep).unwrap();
        assert!(
            ver.is_some(),
            "Expected Some version since rustc is on PATH"
        );
        let v = ver.unwrap();
        assert!(!v.is_empty(), "Expected non-empty version string");
        assert_ne!(v, "xyzzy");
        assert!(v.contains('.'), "Expected version like '1.78.0', got {v}");
    }

    #[test]
    fn rust_is_installed_true_when_rustup_on_path() {
        if which::which("rustup").is_err() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("rust");
        assert!(
            RustModule.is_installed(&pm, &dep).unwrap(),
            "is_installed must be true when rustup is on PATH"
        );
    }

    #[test]
    fn rust_is_installed_false_when_neither_on_path() {
        // Skip if either rustup or cargo is available (realistic dev environment).
        if which::which("rustup").is_ok() || which::which("cargo").is_ok() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("rust");
        assert!(
            !RustModule.is_installed(&pm, &dep).unwrap(),
            "is_installed must be false when neither rustup nor cargo is on PATH"
        );
    }

    #[test]
    fn rust_resolved_version_is_none_when_rustc_absent() {
        if which::which("rustc").is_ok() {
            return;
        }
        if which::which("cargo").is_ok() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("rust");
        let ver = RustModule.resolved_version(&pm, &dep).unwrap();
        assert!(
            ver.is_none(),
            "Expected None version when rustc is not installed"
        );
    }

    #[test]
    fn rust_install_fails_for_invalid_toolchain() {
        // When rustup IS on PATH, calling install with a bad toolchain causes rustup to fail.
        // This kills `replace install -> Ok(())` because the mutation returns Ok((()) unconditionally.
        if which::which("rustup").is_err() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let mut extra = std::collections::HashMap::new();
        extra.insert(
            "toolchain".into(),
            crate::config::ExtraValue::String("invalid-toolchain-devy-test-xyz-12345".into()),
        );
        let dep = Dependency::with_extra("rust", extra);
        let result = RustModule.install(&pm, &dep);
        assert!(
            result.is_err(),
            "install must fail for an invalid toolchain name"
        );
    }

    #[test]
    fn rust_install_rustup_script_failure_propagated() {
        // When rustup is NOT on PATH and the installer script fails, install must return Err.
        // Uses DEVY_TEST_RUSTUP_INSTALLER_SCRIPT to inject a failing script.
        if which::which("rustup").is_ok() {
            return;
        }
        let _guard = crate::test_support::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // SAFETY: serialised by ENV_LOCK; var is only read by RustModule::install.
        unsafe {
            std::env::set_var("DEVY_TEST_RUSTUP_INSTALLER_SCRIPT", "exit 1");
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("rust");
        let result = RustModule.install(&pm, &dep);
        unsafe {
            std::env::remove_var("DEVY_TEST_RUSTUP_INSTALLER_SCRIPT");
        }
        assert!(
            result.is_err(),
            "install must fail when rustup installer exits non-zero"
        );
    }

    #[test]
    fn rust_install_rustup_script_success_does_not_bail() {
        // Kills `delete ! in install` — with mutation, `if status.success()` bails on success.
        // We inject a script that exits 0 (success) and then verify the function
        // proceeds past the bail point (it may still fail at `rustup toolchain install`
        // if rustup is not available, but it must NOT bail with "rustup installation failed").
        if which::which("rustup").is_ok() {
            return;
        }
        let _guard = crate::test_support::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // SAFETY: serialised by ENV_LOCK; var is only read by RustModule::install.
        unsafe {
            std::env::set_var("DEVY_TEST_RUSTUP_INSTALLER_SCRIPT", "exit 0");
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("rust");
        let result = RustModule.install(&pm, &dep);
        unsafe {
            std::env::remove_var("DEVY_TEST_RUSTUP_INSTALLER_SCRIPT");
        }
        // With the `!` deleted, a successful script causes bail! — the error message
        // is "rustup installation failed". A correct implementation proceeds past the
        // bail and fails at the `rustup toolchain install` step (different error).
        if let Err(ref e) = result {
            assert!(
                !e.to_string().contains("rustup installation failed"),
                "install must not bail when the installer script succeeds; got: {e}"
            );
        }
    }

    #[test]
    fn rust_resolved_version_contains_dot_when_rustc_installed() {
        if which::which("rustc").is_err() {
            return;
        }
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("rust");
        let ver = RustModule.resolved_version(&pm, &dep).unwrap();
        if let Some(v) = ver {
            assert!(
                v.contains('.'),
                "Expected semver-like version like '1.78.0', got: {v}"
            );
            assert_ne!(v, "xyzzy", "Version must not be placeholder");
            assert!(!v.is_empty());
        }
    }
}
