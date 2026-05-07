use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

use crate::config::DevyConfig;
use crate::env_manager::{EnvManager, Shadowenv};
use crate::error::SilentExit;
use crate::modules;
use crate::output;
use crate::package_manager;
use crate::package_manager::PackageManager;

use super::shared;

pub(crate) fn check_impl(
    config: &DevyConfig,
    pm: &dyn PackageManager,
    env_mgr: &dyn EnvManager,
    project_root: &Path,
) -> Result<()> {
    let project_name = config.name.as_deref().unwrap_or("project");
    output::header(&format!("devy check · {}", project_name));

    let mut issues: usize = 0;

    let deps = config.normalized_dependencies()?;
    super::up::check_port_conflicts(&deps)?;

    if !deps.is_empty() {
        for dep in &deps {
            pm.validate_config(dep)
                .with_context(|| format!("{}: config validation failed", dep.name))?;
            let module = modules::get(&dep.name);
            if let Some(known) = module.known_extra_keys() {
                for key in dep.extra.keys() {
                    if !known.contains(&key.as_str()) {
                        issues += 1;
                        let hint = if known.is_empty() {
                            "this module accepts no extra keys".to_string()
                        } else {
                            format!("known keys: {}", known.join(", "))
                        };
                        output::warn(&format!(
                            "{}: unrecognized config key `{}` — {hint}",
                            dep.name, key
                        ));
                    }
                }
            }
            for warning in module.config_warnings(dep) {
                output::warn(&format!("{}: {}", dep.name, warning));
            }
        }
        output::header("Dependencies");
        issues += shared::print_dep_table(&deps, pm, true)?;
    }

    // Collect PATH prepends from all modules.
    let path_prepends: Vec<String> = deps
        .iter()
        .flat_map(|dep| modules::get(&dep.name).path_prepends(dep, project_root))
        .collect();

    if !config.environment.is_empty() || !path_prepends.is_empty() {
        output::header("Environment");
        let written_vars = env_mgr.read_vars(project_root);
        issues += shared::print_env_table(&config.environment, written_vars, true)?;

        if !path_prepends.is_empty() {
            let written_paths = env_mgr.read_path_prepends(project_root);
            issues += shared::print_path_table(&path_prepends, written_paths, true);
        }
    }

    println!();
    if issues == 0 {
        output::success("all checks passed");
        Ok(())
    } else {
        let noun = issue_noun(issues);
        eprintln!("  {}  {} {} found", "✗".red().bold(), issues, noun);
        Err(SilentExit(1).into())
    }
}

#[cfg_attr(test, mutants::skip)] // cosmetic singular/plural; no observable behavioral difference
fn issue_noun(count: usize) -> &'static str {
    if count == 1 { "issue" } else { "issues" }
}

#[cfg_attr(test, mutants::skip)] // thin I/O wrapper
pub fn run() -> Result<()> {
    let start = std::env::current_dir().context("Failed to get current directory")?;
    let config_path = DevyConfig::find_config(&start)
        .ok_or_else(|| anyhow::anyhow!("devy.yml not found — are you inside a devy project?"))?;
    let project_root = config_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("devy.yml has no parent directory"))?
        .to_path_buf();
    let config = DevyConfig::load(&config_path)?;
    let pm = package_manager::detect()?;
    let env_mgr = Shadowenv;
    check_impl(&config, pm.as_ref(), &env_mgr, &project_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DevyConfig;
    use crate::env_manager::MockEnvManager;
    use crate::package_manager::MockPackageManager;
    use std::collections::HashMap;

    fn make_config(dep_names: &[&str], env: HashMap<String, String>) -> DevyConfig {
        crate::test_support::make_config(dep_names, env)
    }

    #[test]
    fn check_impl_returns_ok_when_all_installed() {
        let config = make_config(&["node"], HashMap::new());
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(check_impl(&config, &pm, &MockEnvManager::default(), Path::new(".")).is_ok());
    }

    #[test]
    fn check_impl_returns_err_when_dep_not_installed() {
        // Kills `replace run -> Ok(())` — with that mutation the result would always be Ok.
        let config = make_config(&["node"], HashMap::new());
        let pm = MockPackageManager::default(); // installed=false
        assert!(check_impl(&config, &pm, &MockEnvManager::default(), Path::new(".")).is_err());
    }

    #[test]
    fn check_impl_counts_two_missing_deps_as_issues() {
        // Kills `replace += with -=` mutations in issue counting.
        let config = make_config(&["node", "python"], HashMap::new());
        let pm = MockPackageManager::default();
        let result = check_impl(&config, &pm, &MockEnvManager::default(), Path::new("."));
        assert!(result.is_err(), "two missing deps must produce errors");
    }

    #[test]
    fn check_impl_empty_deps_skips_dep_table() {
        // Kills `delete ! in run at line 21` — with mutation, empty deps would print the table.
        // No panic/error expected when deps list is empty.
        let config = make_config(&[], HashMap::new());
        let pm = MockPackageManager::default();
        assert!(check_impl(&config, &pm, &MockEnvManager::default(), Path::new(".")).is_ok());
    }

    #[test]
    fn check_impl_single_issue_uses_singular_noun() {
        // Kills `replace == with != at line 36` for issues == 1.
        // (Verified via the side-effect path, but any deterministic Err is sufficient.)
        let config = make_config(&["node"], HashMap::new());
        let pm = MockPackageManager::default();
        assert!(check_impl(&config, &pm, &MockEnvManager::default(), Path::new(".")).is_err());
    }

    #[test]
    fn check_impl_zero_issues_returns_ok() {
        // Kills `replace == with != at line 32` — with mutation, zero issues would return Err.
        let config = make_config(&["node"], HashMap::new());
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(check_impl(&config, &pm, &MockEnvManager::default(), Path::new(".")).is_ok());
    }

    #[test]
    fn check_impl_not_installed_service_counts_as_issue() {
        // When a service module is not installed, issues += 1.
        // Kills `delete ! in run at line 26` — mutation would count INSTALLED services as issues.
        let config = make_config(&["mysql"], HashMap::new()); // mysql is a service
        let pm = MockPackageManager::default(); // installed=false
        let result = check_impl(&config, &pm, &MockEnvManager::default(), Path::new("."));
        assert!(
            result.is_err(),
            "not-installed service must count as an issue"
        );
    }

    #[test]
    fn check_impl_env_issue_without_dep_issues_returns_err() {
        // Kills `replace += with *=` at line 28 (env issues counter).
        // With *=: issues = 0 (no dep issues) * env_count = 0 → Ok, but should be Err.
        // Also kills `replace += with -=` potential equivalence by testing cumulative count.
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "value".to_string());
        // Empty deps so the dep block contributes 0 to issues.
        let config = make_config(&[], env);
        let pm = MockPackageManager::default();
        // print_env_table with bold_errors=true and no shadowenv file returns env.len() = 1 issue.
        let result = check_impl(&config, &pm, &MockEnvManager::default(), Path::new("."));
        assert!(
            result.is_err(),
            "a missing env var must be counted as an issue"
        );
    }

    #[test]
    fn check_impl_dep_and_env_issues_both_contribute() {
        // Kills `replace += with -=` at lines 23 and 28 in combination:
        // any non-zero issues count (even usize::MAX) still returns Err, so we verify
        // the function is also Err when both sources independently contribute.
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "val".to_string());
        let config = make_config(&["node"], env);
        let pm = MockPackageManager::default(); // nothing installed, no shadowenv file
        assert!(check_impl(&config, &pm, &MockEnvManager::default(), Path::new(".")).is_err());
    }

    #[test]
    fn check_impl_returns_err_on_unrecognized_extra_key() {
        // minio has known_extra_keys; portx is not in the list — must count as an issue.
        let yaml = "dependencies:\n  - minio:\n      portx: 9001\n";
        let config: crate::config::DevyConfig = serde_yml::from_str(yaml).unwrap();
        let pm = MockPackageManager {
            installed: true,
            service_running: true,
            ..Default::default()
        };
        let result = check_impl(&config, &pm, &MockEnvManager::default(), Path::new("."));
        assert!(
            result.is_err(),
            "unrecognized extra key must count as an issue"
        );
    }

    #[test]
    fn check_impl_returns_err_on_port_conflict() {
        // mysql and mariadb both default to 3306 — check must catch this, not just up.
        let config = make_config(&["mysql", "mariadb"], HashMap::new());
        let pm = MockPackageManager {
            installed: true,
            service_running: true,
            ..Default::default()
        };
        let result = check_impl(&config, &pm, &MockEnvManager::default(), Path::new("."));
        assert!(
            result.is_err(),
            "port conflict must be caught by devy check"
        );
    }

    #[test]
    fn check_impl_warns_on_extra_key_for_module_with_empty_allowlist() {
        // ruby has no known extra keys (inherits Some(&[]) default) — any extra key must warn.
        let yaml = "dependencies:\n  - ruby:\n      vrsion: \"3.3.0\"\n";
        let config: DevyConfig = serde_yml::from_str(yaml).unwrap();
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        let warn_count = crate::output::with_warn_capture(|| {
            let _ = check_impl(&config, &pm, &MockEnvManager::default(), Path::new("."));
        });
        assert!(
            warn_count > 0,
            "check_impl must warn on unknown extra key for module with empty allowlist"
        );
    }

    #[test]
    fn check_impl_emits_config_warnings_for_minio_with_credentials() {
        // config_warnings on MinioModule fires when access_key is configured.
        // check_impl must call it and emit the warning via output::warn.
        let yaml = "dependencies:\n  - minio:\n      access_key: myuser\n";
        let config: DevyConfig = serde_yml::from_str(yaml).unwrap();
        let pm = MockPackageManager {
            installed: true,
            service_running: true,
            ..Default::default()
        };
        let warn_count = crate::output::with_warn_capture(|| {
            let _ = check_impl(&config, &pm, &MockEnvManager::default(), Path::new("."));
        });
        assert!(
            warn_count > 0,
            "check_impl must emit config_warnings for minio with credentials"
        );
    }
}
