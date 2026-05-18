use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::{Command, Stdio};
use which::which;

use crate::config::Dependency;
use crate::output;
use crate::package_manager::PackageManager;

use super::Module;
use super::helpers::{stamp_matches, write_stamp};

pub struct DenoModule;

fn deno_bin() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(|home| format!("{home}/.deno/bin/deno"))
}

impl Module for DenoModule {
    fn source(&self) -> Option<&'static str> {
        Some("deno-installer")
    }

    fn nix_attr(&self, _dep: &Dependency) -> Option<String> {
        Some("deno".to_string())
    }

    fn is_installed(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<bool> {
        let bin_exists = deno_bin()
            .map(|b| std::path::Path::new(&b).exists())
            .unwrap_or(false);
        Ok(which("deno").is_ok() || bin_exists)
    }

    fn install(&self, _pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        #[cfg(test)]
        let base_script = std::env::var("ENVY_TEST_DENO_INSTALL_SCRIPT")
            .unwrap_or_else(|_| "curl -fsSL https://deno.land/install.sh | sh".to_string());
        #[cfg(not(test))]
        let base_script = "curl -fsSL https://deno.land/install.sh | sh".to_string();

        let version_arg = dep.version.as_deref().map(|v| {
            if v.starts_with('v') {
                v.to_string()
            } else {
                format!("v{v}")
            }
        });

        let mut cmd = Command::new("sh");
        cmd.arg("-c");
        if let Some(ref ver) = version_arg {
            // The installer script accepts the version as its first positional argument.
            cmd.arg(format!("{base_script} -s {ver}"));
        } else {
            cmd.arg(&base_script);
        }

        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| anyhow::anyhow!("Failed to run Deno installer: {e}"))?;

        if !status.success() {
            bail!("Deno installation failed — check the output above for details");
        }
        Ok(())
    }

    fn post_setup(
        &self,
        _dep: &Dependency,
        _pm: &dyn PackageManager,
        project_root: &Path,
    ) -> Result<()> {
        let deno_json = project_root.join("deno.json");
        let deno_jsonc = project_root.join("deno.jsonc");
        let manifest = if deno_json.exists() {
            Some(deno_json)
        } else if deno_jsonc.exists() {
            Some(deno_jsonc)
        } else {
            None
        };
        let Some(manifest) = manifest else {
            return Ok(());
        };
        let stamp_path = project_root.join(".devy_deno_stamp");
        if stamp_matches(&stamp_path, &manifest) {
            output::skip("Deno dependencies up to date");
            return Ok(());
        }
        let deno = deno_bin()
            .filter(|b| std::path::Path::new(b).exists())
            .unwrap_or_else(|| "deno".into());
        output::step("Running deno install");
        let status = Command::new(&deno)
            .arg("install")
            .current_dir(project_root)
            .status()
            .context("Failed to run `deno install`")?;
        if !status.success() {
            anyhow::bail!("`deno install` failed — check the output above for details");
        }
        write_stamp(&stamp_path, &manifest);
        output::success("Deno dependencies installed");
        Ok(())
    }

    fn resolved_version(
        &self,
        _pm: &dyn PackageManager,
        _dep: &Dependency,
    ) -> Result<Option<String>> {
        let path = deno_bin()
            .filter(|b| std::path::Path::new(b).exists())
            .unwrap_or_else(|| "deno".to_string());
        let out = Command::new(path).arg("--version").output();
        // "deno 1.40.0 ..." — take second token of first line
        Ok(out.ok().and_then(|o| {
            String::from_utf8(o.stdout)
                .ok()?
                .lines()
                .next()?
                .split_whitespace()
                .nth(1)
                .map(String::from)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;

    #[test]
    fn deno_module_is_not_a_service() {
        assert!(!DenoModule.is_service());
    }

    #[test]
    fn deno_source_is_deno_installer() {
        assert_eq!(DenoModule.source(), Some("deno-installer"));
    }

    #[test]
    fn deno_bin_returns_some_with_non_empty_home() {
        if std::env::var("HOME")
            .map(|h| !h.is_empty())
            .unwrap_or(false)
        {
            let bin = deno_bin().unwrap();
            assert!(
                bin.contains(".deno/bin/deno"),
                "Expected ~/.deno/bin/deno, got {bin}"
            );
        }
    }

    #[test]
    fn deno_bin_path_contains_home() {
        if let Ok(home) = std::env::var("HOME")
            && !home.is_empty()
        {
            let bin = deno_bin().unwrap();
            assert!(bin.starts_with(&home));
        }
    }

    #[test]
    fn deno_is_installed_does_not_panic() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("deno");
        let _ = DenoModule.is_installed(&pm, &dep);
    }

    #[test]
    fn deno_is_installed_true_when_bin_file_exists_and_not_on_path() {
        // Only runs when deno is NOT on PATH.
        if which("deno").is_ok() {
            return;
        }
        if let Some(bin_path) = deno_bin() {
            let path = std::path::Path::new(&bin_path);
            if path.exists() {
                return;
            }
            std::fs::create_dir_all(path.parent().unwrap()).ok();
            std::fs::write(&bin_path, b"#!/bin/sh").ok();
            let pm = MockPackageManager::default();
            let dep = Dependency::simple("deno");
            let result = DenoModule.is_installed(&pm, &dep).unwrap();
            std::fs::remove_file(&bin_path).ok();
            assert!(
                result,
                "Expected is_installed=true when deno binary exists at {bin_path}"
            );
        }
    }

    #[test]
    fn deno_resolved_version_returns_ok() {
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("deno");
        assert!(DenoModule.resolved_version(&pm, &dep).is_ok());
    }

    #[test]
    fn deno_resolved_version_is_non_empty_when_installed() {
        if which("deno").is_err() {
            return;
        }
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("deno");
        let ver = DenoModule.resolved_version(&pm, &dep).unwrap();
        assert!(ver.is_some(), "Expected Some version when deno is on PATH");
        assert!(
            !ver.unwrap().is_empty(),
            "Expected non-empty version string"
        );
    }

    #[test]
    fn deno_is_installed_false_when_not_on_path_and_no_bin_file() {
        if which("deno").is_ok() {
            return;
        }
        if let Some(p) = deno_bin()
            && std::path::Path::new(&p).exists()
        {
            return;
        }
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("deno");
        assert!(
            !DenoModule.is_installed(&pm, &dep).unwrap(),
            "is_installed must return false when deno is absent from PATH and no bin file"
        );
    }

    #[test]
    fn deno_resolved_version_is_none_when_not_installed() {
        if which("deno").is_ok() {
            return;
        }
        if let Some(p) = deno_bin()
            && std::path::Path::new(&p).exists()
        {
            return;
        }
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("deno");
        let ver = DenoModule.resolved_version(&pm, &dep).unwrap();
        assert!(
            ver.is_none(),
            "Expected None version when deno is not installed"
        );
    }

    #[test]
    fn deno_install_fails_when_script_exits_nonzero() {
        // Uses ENVY_TEST_DENO_INSTALL_SCRIPT to inject a failing script.
        let _guard = crate::test_support::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // SAFETY: serialised by ENV_LOCK; var is only read by DenoModule::install.
        unsafe {
            std::env::set_var("ENVY_TEST_DENO_INSTALL_SCRIPT", "exit 1");
        }
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("deno");
        let result = DenoModule.install(&pm, &dep);
        unsafe {
            std::env::remove_var("ENVY_TEST_DENO_INSTALL_SCRIPT");
        }
        assert!(
            result.is_err(),
            "install must return Err when script exits non-zero"
        );
    }

    fn file_mtime_secs(path: &std::path::Path) -> u64 {
        std::fs::metadata(path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn deno_post_setup_no_config_file_is_noop() {
        let dir = crate::test_support::tmp_dir();
        let pm = MockPackageManager::default();
        assert!(
            DenoModule
                .post_setup(&Dependency::simple("deno"), &pm, &dir)
                .is_ok()
        );
    }

    #[test]
    fn deno_post_setup_skips_install_when_stamp_matches_deno_json() {
        let dir = crate::test_support::tmp_dir();
        let cfg = dir.join("deno.json");
        std::fs::write(&cfg, r#"{"imports":{}}"#).unwrap();
        std::fs::write(
            dir.join(".devy_deno_stamp"),
            file_mtime_secs(&cfg).to_string(),
        )
        .unwrap();
        let pm = MockPackageManager::default();
        assert!(
            DenoModule
                .post_setup(&Dependency::simple("deno"), &pm, &dir)
                .is_ok()
        );
    }

    #[test]
    fn deno_post_setup_skips_install_when_stamp_matches_deno_jsonc() {
        let dir = crate::test_support::tmp_dir();
        let cfg = dir.join("deno.jsonc");
        std::fs::write(&cfg, "{}").unwrap();
        std::fs::write(
            dir.join(".devy_deno_stamp"),
            file_mtime_secs(&cfg).to_string(),
        )
        .unwrap();
        let pm = MockPackageManager::default();
        assert!(
            DenoModule
                .post_setup(&Dependency::simple("deno"), &pm, &dir)
                .is_ok()
        );
    }

    #[test]
    fn deno_post_setup_prefers_deno_json_over_deno_jsonc() {
        let dir = crate::test_support::tmp_dir();
        let json = dir.join("deno.json");
        std::fs::write(&json, "{}").unwrap();
        std::fs::write(dir.join("deno.jsonc"), "{}").unwrap();
        std::fs::write(
            dir.join(".devy_deno_stamp"),
            file_mtime_secs(&json).to_string(),
        )
        .unwrap();
        let pm = MockPackageManager::default();
        assert!(
            DenoModule
                .post_setup(&Dependency::simple("deno"), &pm, &dir)
                .is_ok()
        );
    }

    #[test]
    fn deno_resolved_version_contains_dot_when_installed() {
        if which("deno").is_err() {
            if let Some(p) = deno_bin() {
                if !std::path::Path::new(&p).exists() {
                    return;
                }
            } else {
                return;
            }
        }
        let pm = MockPackageManager::default();
        let dep = Dependency::simple("deno");
        let ver = DenoModule.resolved_version(&pm, &dep).unwrap();
        if let Some(v) = ver {
            assert!(v.contains('.'), "Expected semver-like version, got: {v}");
            assert_ne!(v, "xyzzy", "Version must not be the placeholder 'xyzzy'");
        }
    }
}
