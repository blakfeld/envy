use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::config::Dependency;
use crate::output;
use crate::package_manager::PackageManager;

use super::{Module, pm_dep};

pub struct PythonModule;

fn pkg_name(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "apt" => "python3",
        "winget" => "Python.Python.3",
        _ => "python",
    }
}

fn venv_path(dep: &Dependency) -> String {
    dep.extra
        .get("venv_path")
        .and_then(|v| v.as_str())
        .unwrap_or(".venv")
        .to_string()
}

fn venv_bin_dir(_venv_dir: &Path) -> &'static str {
    if cfg!(target_os = "windows") {
        "Scripts"
    } else {
        "bin"
    }
}

fn detect_install_cmd(dep: &Dependency, project_root: &Path, venv_dir: &Path) -> Option<String> {
    if let Some(cmd) = dep.extra.get("install_cmd").and_then(|v| v.as_str()) {
        return Some(cmd.to_string());
    }
    let pip = venv_dir.join(venv_bin_dir(venv_dir)).join("pip");
    let pip = pip.display();
    if project_root.join("pyproject.toml").exists() {
        return Some(format!("{pip} install -e ."));
    }
    if project_root.join("requirements.txt").exists() {
        return Some(format!("{pip} install -r requirements.txt"));
    }
    None
}

impl Module for PythonModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, pkg_name(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, pkg_name(pm)))
    }

    fn env_vars(&self, dep: &Dependency) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        if let Ok(cwd) = std::env::current_dir() {
            let abs_venv = cwd.join(venv_path(dep));
            vars.insert("VIRTUAL_ENV".into(), abs_venv.display().to_string());
        }
        vars
    }

    fn path_prepends(&self, dep: &Dependency) -> Vec<String> {
        std::env::current_dir()
            .ok()
            .map(|cwd| {
                let venv_dir = cwd.join(venv_path(dep));
                vec![venv_dir.join(venv_bin_dir(&venv_dir)).display().to_string()]
            })
            .unwrap_or_default()
    }

    fn post_setup(&self, dep: &Dependency, project_root: &Path) -> Result<()> {
        let venv_dir = project_root.join(venv_path(dep));

        if !venv_dir.join("pyvenv.cfg").exists() {
            output::step("Creating Python virtualenv");
            let status = Command::new("python3")
                .args(["-m", "venv", venv_dir.to_str().unwrap_or(".venv")])
                .status()
                .context("Failed to create Python virtualenv")?;
            if !status.success() {
                anyhow::bail!("`python3 -m venv` failed");
            }
            output::success("Virtualenv created");
        }

        let Some(cmd) = detect_install_cmd(dep, project_root, &venv_dir) else {
            return Ok(());
        };

        output::step(&format!("Running {cmd}"));
        let status = Command::new("sh")
            .args(["-c", &cmd])
            .current_dir(project_root)
            .status()
            .with_context(|| format!("Failed to run `{cmd}`"))?;
        if !status.success() {
            anyhow::bail!("`{cmd}` failed");
        }
        output::success("Python dependencies installed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;
    use std::collections::HashMap;

    #[test]
    fn python_is_not_a_service() {
        assert!(!PythonModule.is_service());
    }

    #[test]
    fn pkg_name_apt() {
        let pm = MockPackageManager {
            name: "apt",
            ..Default::default()
        };
        assert_eq!(pkg_name(&pm), "python3");
    }

    #[test]
    fn pkg_name_winget() {
        let pm = MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(pkg_name(&pm), "Python.Python.3");
    }

    #[test]
    fn pkg_name_brew() {
        let pm = MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(pkg_name(&pm), "python");
    }

    #[test]
    fn python_is_installed_delegates_to_pm() {
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(
            PythonModule
                .is_installed(&pm, &Dependency::simple("python"))
                .unwrap()
        );
    }

    #[test]
    fn python_not_installed_when_pm_reports_false() {
        let pm = MockPackageManager::default();
        assert!(
            !PythonModule
                .is_installed(&pm, &Dependency::simple("python"))
                .unwrap()
        );
    }

    #[test]
    fn python_install_propagates_pm_error() {
        let pm = MockPackageManager {
            install_fails: true,
            ..Default::default()
        };
        assert!(
            PythonModule
                .install(&pm, &Dependency::simple("python"))
                .is_err()
        );
    }

    #[test]
    fn venv_path_default_is_dotenv() {
        let dep = Dependency::simple("python");
        assert_eq!(venv_path(&dep), ".venv");
    }

    #[test]
    fn venv_path_custom_from_extra() {
        let mut extra = HashMap::new();
        extra.insert("venv_path".into(), serde_yaml::Value::String("venv".into()));
        let dep = Dependency {
            name: "python".into(),
            version: None,
            tap: None,
            after_install: None,
            extra,
        };
        assert_eq!(venv_path(&dep), "venv");
    }

    #[test]
    fn python_env_vars_contains_virtual_env() {
        let dep = Dependency::simple("python");
        let vars = PythonModule.env_vars(&dep);
        assert!(vars.contains_key("VIRTUAL_ENV"), "VIRTUAL_ENV must be set");
        assert!(vars["VIRTUAL_ENV"].ends_with("/.venv") || vars["VIRTUAL_ENV"].contains(".venv"));
    }

    #[test]
    fn python_path_prepends_points_into_venv() {
        let dep = Dependency::simple("python");
        let prepends = PythonModule.path_prepends(&dep);
        assert_eq!(prepends.len(), 1);
        assert!(prepends[0].contains(".venv"));
    }

    #[test]
    fn post_setup_no_requirements_no_pyproject_is_noop() {
        // A temp dir with no requirements.txt or pyproject.toml and an existing
        // pyvenv.cfg — post_setup must return Ok without running pip.
        let dir = std::env::temp_dir().join(format!("devy_python_test_{}", std::process::id()));
        let venv = dir.join(".venv");
        std::fs::create_dir_all(&venv).unwrap();
        std::fs::write(venv.join("pyvenv.cfg"), "").unwrap();

        let dep = Dependency::simple("python");
        assert!(PythonModule.post_setup(&dep, &dir).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_install_cmd_explicit_override() {
        let mut extra = HashMap::new();
        extra.insert(
            "install_cmd".into(),
            serde_yaml::Value::String("pip install mypackage".into()),
        );
        let dep = Dependency {
            name: "python".into(),
            version: None,
            tap: None,
            after_install: None,
            extra,
        };
        let dir = std::env::temp_dir();
        let venv = dir.join(".venv");
        let cmd = detect_install_cmd(&dep, &dir, &venv);
        assert_eq!(cmd.as_deref(), Some("pip install mypackage"));
    }

    #[test]
    fn detect_install_cmd_requirements_txt() {
        let dir = std::env::temp_dir().join(format!("devy_python_cmd_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("requirements.txt"), "").unwrap();

        let dep = Dependency::simple("python");
        let venv = dir.join(".venv");
        let cmd = detect_install_cmd(&dep, &dir, &venv);
        assert!(cmd.unwrap().contains("requirements.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_install_cmd_pyproject_toml_preferred_over_requirements() {
        let dir =
            std::env::temp_dir().join(format!("devy_python_pyproject_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pyproject.toml"), "").unwrap();
        std::fs::write(dir.join("requirements.txt"), "").unwrap();

        let dep = Dependency::simple("python");
        let venv = dir.join(".venv");
        let cmd = detect_install_cmd(&dep, &dir, &venv);
        assert!(
            cmd.unwrap().contains("-e ."),
            "pyproject.toml must take priority"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
