use anyhow::{Result, bail};
use std::process::{Command, Stdio};
use which::which;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

use super::Module;

pub struct BunModule;

fn bun_bin() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(|home| format!("{home}/.bun/bin/bun"))
}

impl Module for BunModule {
    fn source(&self) -> Option<&'static str> {
        Some("bun-installer")
    }

    fn is_installed(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<bool> {
        let bin_exists = bun_bin()
            .map(|b| std::path::Path::new(&b).exists())
            .unwrap_or(false);
        Ok(which("bun").is_ok() || bin_exists)
    }

    fn install(&self, _pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        #[cfg(test)]
        let script = std::env::var("ENVY_TEST_BUN_INSTALL_SCRIPT")
            .unwrap_or_else(|_| "curl -fsSL https://bun.sh/install | bash".to_string());
        #[cfg(not(test))]
        let script = "curl -fsSL https://bun.sh/install | bash".to_string();

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&script);

        if let Some(ver) = dep.version.as_deref() {
            // BUN_INSTALL_VERSION controls which release the script fetches.
            cmd.env("BUN_INSTALL_VERSION", ver);
        }

        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| anyhow::anyhow!("Failed to run Bun installer: {e}"))?;

        if !status.success() {
            bail!("Bun installation failed");
        }
        Ok(())
    }

    fn resolved_version(
        &self,
        _pm: &dyn PackageManager,
        _dep: &Dependency,
    ) -> Result<Option<String>> {
        let path = bun_bin()
            .filter(|b| std::path::Path::new(b).exists())
            .unwrap_or_else(|| "bun".to_string());
        let out = Command::new(path).arg("--version").output();
        // "1.0.25" — output is just the version string
        Ok(out.ok().and_then(|o| {
            String::from_utf8(o.stdout)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;

    #[test]
    fn bun_module_is_not_a_service() {
        assert!(!BunModule.is_service());
    }

    #[test]
    fn bun_source_is_bun_installer() {
        assert_eq!(BunModule.source(), Some("bun-installer"));
    }

    #[test]
    fn bun_bin_returns_some_with_non_empty_home() {
        // On any normal system HOME is set and non-empty, so bun_bin() should return Some.
        if std::env::var("HOME")
            .map(|h| !h.is_empty())
            .unwrap_or(false)
        {
            let bin = bun_bin().unwrap();
            assert!(
                bin.contains(".bun/bin/bun"),
                "Expected ~/.bun/bin/bun, got {bin}"
            );
        }
    }

    #[test]
    fn bun_bin_path_contains_home() {
        if let Ok(home) = std::env::var("HOME")
            && !home.is_empty()
        {
            let bin = bun_bin().unwrap();
            assert!(bin.starts_with(&home));
        }
    }

    #[test]
    fn bun_is_installed_does_not_panic() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("bun");
        let _ = BunModule.is_installed(&pm, &dep);
    }

    #[test]
    fn bun_is_installed_true_when_bin_file_exists_and_not_on_path() {
        // Only runs when bun is NOT on PATH (to avoid modifying real installations).
        if which("bun").is_ok() {
            return;
        }
        if let Some(bin_path) = bun_bin() {
            let path = std::path::Path::new(&bin_path);
            if path.exists() {
                return;
            }
            std::fs::create_dir_all(path.parent().unwrap()).ok();
            std::fs::write(&bin_path, b"#!/bin/sh").ok();
            let pm = MockPackageManager::default();
            let dep = Dependency::simple("bun");
            let result = BunModule.is_installed(&pm, &dep).unwrap();
            std::fs::remove_file(&bin_path).ok();
            assert!(
                result,
                "Expected is_installed=true when bun binary exists at {bin_path}"
            );
        }
    }

    #[test]
    fn bun_resolved_version_returns_ok() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("bun");
        // Must not error regardless of whether bun is installed.
        assert!(BunModule.resolved_version(&pm, &dep).is_ok());
    }

    #[test]
    fn bun_resolved_version_is_non_empty_when_installed() {
        if which("bun").is_err() {
            return;
        }
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("bun");
        let ver = BunModule.resolved_version(&pm, &dep).unwrap();
        assert!(ver.is_some(), "Expected Some version when bun is on PATH");
        assert!(
            !ver.unwrap().is_empty(),
            "Expected non-empty version string"
        );
    }

    #[test]
    fn bun_is_installed_false_when_not_on_path_and_no_bin_file() {
        // Skip if bun is reachable via PATH — can't test the false case then.
        if which("bun").is_ok() {
            return;
        }
        // Also skip if the bun_bin() path already exists (e.g. ~/.bun/bin/bun).
        if let Some(p) = bun_bin()
            && std::path::Path::new(&p).exists()
        {
            return;
        }
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("bun");
        assert!(
            !BunModule.is_installed(&pm, &dep).unwrap(),
            "is_installed must return false when bun is absent from PATH and no bin file"
        );
    }

    #[test]
    fn bun_resolved_version_is_none_when_bun_not_installed() {
        if which("bun").is_ok() {
            return;
        }
        if let Some(p) = bun_bin()
            && std::path::Path::new(&p).exists()
        {
            return;
        }
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("bun");
        let ver = BunModule.resolved_version(&pm, &dep).unwrap();
        assert!(
            ver.is_none(),
            "Expected None version when bun is not installed"
        );
    }

    #[test]
    fn bun_install_fails_when_script_exits_nonzero() {
        // Uses ENVY_TEST_BUN_INSTALL_SCRIPT to inject a failing script.
        let _guard = crate::test_support::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // SAFETY: serialised by ENV_LOCK; var is only read by BunModule::install.
        unsafe {
            std::env::set_var("ENVY_TEST_BUN_INSTALL_SCRIPT", "exit 1");
        }
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("bun");
        let result = BunModule.install(&pm, &dep);
        unsafe {
            std::env::remove_var("ENVY_TEST_BUN_INSTALL_SCRIPT");
        }
        assert!(
            result.is_err(),
            "install must return Err when script exits non-zero"
        );
    }

    #[test]
    fn bun_resolved_version_contains_dot_when_installed() {
        if which("bun").is_err() {
            if let Some(p) = bun_bin() {
                if !std::path::Path::new(&p).exists() {
                    return;
                }
            } else {
                return;
            }
        }
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("bun");
        let ver = BunModule.resolved_version(&pm, &dep).unwrap();
        if let Some(v) = ver {
            assert!(v.contains('.'), "Expected semver-like version, got: {v}");
            assert_ne!(v, "xyzzy", "Version must not be the placeholder 'xyzzy'");
        }
    }
}
