use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;
use std::path::Path;

use crate::config::Dependency;
use crate::env_manager::shadowenv;
use crate::modules;
use crate::output;
use crate::package_manager::PackageManager;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manager::MockPackageManager;

    #[test]
    fn print_dep_table_returns_zero_when_all_installed() {
        let pm = MockPackageManager { installed: true, ..Default::default() };
        let deps = vec![Dependency::simple("node"), Dependency::simple("python")];
        let issues = print_dep_table(&deps, &pm, false).unwrap();
        assert_eq!(issues, 0, "no issues when all deps are installed");
    }

    #[test]
    fn print_dep_table_returns_nonzero_when_dep_missing() {
        // Kills `replace print_dep_table -> Ok(0)` and `replace -> Ok(1)`.
        let pm = MockPackageManager::default(); // installed=false
        let deps = vec![Dependency::simple("node"), Dependency::simple("python")];
        let issues = print_dep_table(&deps, &pm, false).unwrap();
        assert!(issues > 0, "should count missing deps as issues");
    }

    #[test]
    fn print_dep_table_counts_each_missing_dep() {
        // Kills `replace += with -=` — with subtraction, issues would be negative (wraps to usize::MAX).
        let pm = MockPackageManager::default();
        let deps = vec![
            Dependency::simple("node"),
            Dependency::simple("python"),
            Dependency::simple("ruby"),
        ];
        let issues = print_dep_table(&deps, &pm, false).unwrap();
        assert_eq!(issues, 3, "each missing dep must add 1 to issues");
    }

    #[test]
    fn print_dep_table_empty_deps_returns_zero() {
        // Kills `delete ! in print_dep_table at line 48` — with mutation, empty list would
        // enter the service-running check and panic (service running check on non-service).
        // Actually, empty deps → no iterations → issues = 0.
        let pm = MockPackageManager::default();
        let issues = print_dep_table(&[], &pm, false).unwrap();
        assert_eq!(issues, 0);
    }

    #[test]
    fn print_dep_table_counts_stopped_service_as_issue() {
        // Kills `replace += with -=` at line 53 (service stopped counter).
        let pm = MockPackageManager { installed: true, service_running: false, ..Default::default() };
        let deps = vec![Dependency::simple("mysql")]; // mysql is a service
        let issues = print_dep_table(&deps, &pm, false).unwrap();
        assert_eq!(issues, 1, "stopped service must count as one issue");
    }

    #[test]
    fn print_dep_table_running_service_does_not_add_issues() {
        let pm = MockPackageManager { installed: true, service_running: true, ..Default::default() };
        let deps = vec![Dependency::simple("mysql")];
        let issues = print_dep_table(&deps, &pm, false).unwrap();
        assert_eq!(issues, 0, "running service must not add to issues");
    }

    #[test]
    fn print_dep_table_uninstalled_service_counts_only_once() {
        // Kills `delete ! in print_dep_table` at the `!installed` check for service rendering.
        // When not installed, service status shows "–" (not checked), so only 1 issue.
        let pm = MockPackageManager::default(); // installed=false, service_running=false
        let deps = vec![Dependency::simple("mysql")];
        let issues = print_dep_table(&deps, &pm, false).unwrap();
        assert_eq!(issues, 1, "uninstalled service should count as exactly 1 issue");
    }

    #[test]
    fn print_env_table_returns_zero_for_empty_env() {
        let issues = print_env_table(&HashMap::new(), false).unwrap();
        assert_eq!(issues, 0);
    }

    #[test]
    fn print_env_table_check_mode_returns_count_when_env_not_configured() {
        // Kills `replace print_env_table -> Ok(0)` and `replace -> Ok(1)`.
        // With bold_errors=true (check mode) and no shadowenv file, issues = config_env.len().
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        env.insert("BAZ".to_string(), "qux".to_string());
        let issues = print_env_table(&env, true).unwrap();
        assert_eq!(issues, 2, "issues must equal env count when shadowenv file is missing");
    }

    #[test]
    fn print_env_table_status_mode_returns_zero_when_not_configured() {
        // In status mode (bold_errors=false), missing config prints a message but counts 0 issues.
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        let issues = print_env_table(&env, false).unwrap();
        assert_eq!(issues, 0);
    }

    #[test]
    fn print_env_table_bold_errors_guard_distinguishes_modes() {
        // Kills `replace match guard bold_errors with true/false` mutations.
        let mut env = HashMap::new();
        env.insert("KEY".to_string(), "val".to_string());
        // true mode reports issues; false mode reports 0 (status mode).
        let check_issues = print_env_table(&env, true).unwrap();
        let status_issues = print_env_table(&env, false).unwrap();
        assert!(check_issues >= status_issues,
            "check mode issues ({check_issues}) must be >= status mode issues ({status_issues})");
    }
}

/// Renders the dependency status table and returns the number of issues found.
/// Pass `bold_errors = true` when failures should be displayed in bold (check command).
pub fn print_dep_table(
    deps: &[Dependency],
    pm: &dyn PackageManager,
    bold_errors: bool,
) -> Result<usize> {
    let name_col = deps
        .iter()
        .map(|d| d.versioned_name().len())
        .max()
        .unwrap_or(0);
    let status_col = "not installed".len();
    let mut issues = 0usize;

    for dep in deps {
        let module = modules::get(&dep.name);
        let name = dep.versioned_name();
        let installed = module.is_installed(pm, dep)?;

        let (icon, status) = if installed {
            (
                "✓".green().bold().to_string(),
                "installed".green().to_string(),
            )
        } else {
            issues += 1;
            let txt = if bold_errors {
                "not installed".red().bold().to_string()
            } else {
                "not installed".red().to_string()
            };
            ("✗".red().bold().to_string(), txt)
        };

        let service = if module.is_service() {
            if !installed {
                "–".dimmed().to_string()
            } else if module.is_running(pm, dep)? {
                format!("{} {}", "✓".green().bold(), "running".green())
            } else {
                issues += 1;
                let txt = if bold_errors {
                    "stopped".red().bold().to_string()
                } else {
                    "stopped".red().to_string()
                };
                format!("{} {}", "✗".red().bold(), txt)
            }
        } else {
            "–".dimmed().to_string()
        };

        println!(
            "  {}  {:<name_col$}  {:<status_col$}  {}",
            icon, name, status, service
        );
    }

    Ok(issues)
}

/// Renders the environment variable status table and returns the number of issues found.
///
/// When `bold_errors = true` (check command): shows ✓/✗ icons with "configured"/"missing" labels.
/// When `bold_errors = false` (status command): shows each key with its current value.
pub fn print_env_table(
    config_env: &HashMap<String, String>,
    bold_errors: bool,
) -> Result<usize> {
    let mut issues = 0usize;
    let written = shadowenv::read_vars(Path::new(shadowenv::ENV_FILE));
    let key_col = config_env.keys().map(|k| k.len()).max().unwrap_or(0);

    match written {
        None if bold_errors => {
            issues += config_env.len();
            println!(
                "  {}  environment not configured — run {}",
                "✗".red().bold(),
                "envy up".bold()
            );
        }
        None => {
            output::info("not configured — run envy up first");
        }
        Some(ref vars) if bold_errors => {
            for key in config_env.keys() {
                let (icon, value): (String, String) = if vars.contains_key(key) {
                    (
                        "✓".green().bold().to_string(),
                        "configured".green().to_string(),
                    )
                } else {
                    issues += 1;
                    (
                        "✗".red().bold().to_string(),
                        "missing".red().bold().to_string(),
                    )
                };
                println!("  {}  {:<key_col$}  {}", icon, key, value);
            }
        }
        Some(vars) => {
            for key in config_env.keys() {
                let value = vars
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or("(not set)")
                    .dimmed();
                println!("  {:<key_col$}  {}", key, value);
            }
        }
    }

    Ok(issues)
}
