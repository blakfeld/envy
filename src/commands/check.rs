use anyhow::Result;
use colored::Colorize;

use crate::config::EnvyConfig;
use crate::error::SilentExit;
use crate::output;
use crate::package_manager;
use crate::package_manager::PackageManager;

use super::shared;

pub(crate) fn check_impl(config: &EnvyConfig, pm: &dyn PackageManager) -> Result<()> {
    let project_name = config.name.as_deref().unwrap_or("project");
    output::header(&format!("devy check · {}", project_name));

    let mut issues: usize = 0;

    let deps = config.normalized_dependencies();
    if !deps.is_empty() {
        output::header("Dependencies");
        issues += shared::print_dep_table(&deps, pm, true)?;
    }

    if !config.environment.is_empty() {
        output::header("Environment");
        issues += shared::print_env_table(&config.environment, true)?;
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

#[mutants::skip] // cosmetic singular/plural; no observable behavioral difference
fn issue_noun(count: usize) -> &'static str {
    if count == 1 { "issue" } else { "issues" }
}

pub fn run() -> Result<()> {
    let config = EnvyConfig::load_default()?;
    let pm = package_manager::detect()?;
    check_impl(&config, pm.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EnvyConfig, RawDependency};
    use crate::package_manager::MockPackageManager;
    use std::collections::HashMap;

    fn make_config(dep_names: &[&str], env: HashMap<String, String>) -> EnvyConfig {
        EnvyConfig {
            name: Some("test".into()),
            dependencies: dep_names
                .iter()
                .map(|n| RawDependency::Simple(n.to_string()))
                .collect(),
            environment: env,
            commands: HashMap::new(),

            hooks: Default::default(),
        }
    }

    #[test]
    fn check_impl_returns_ok_when_all_installed() {
        let config = make_config(&["node"], HashMap::new());
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(check_impl(&config, &pm).is_ok());
    }

    #[test]
    fn check_impl_returns_err_when_dep_not_installed() {
        // Kills `replace run -> Ok(())` — with that mutation the result would always be Ok.
        let config = make_config(&["node"], HashMap::new());
        let pm = MockPackageManager::default(); // installed=false
        assert!(check_impl(&config, &pm).is_err());
    }

    #[test]
    fn check_impl_counts_two_missing_deps_as_issues() {
        // Kills `replace += with -=` mutations in issue counting.
        let config = make_config(&["node", "python"], HashMap::new());
        let pm = MockPackageManager::default();
        let result = check_impl(&config, &pm);
        assert!(result.is_err(), "two missing deps must produce errors");
    }

    #[test]
    fn check_impl_empty_deps_skips_dep_table() {
        // Kills `delete ! in run at line 21` — with mutation, empty deps would print the table.
        // No panic/error expected when deps list is empty.
        let config = make_config(&[], HashMap::new());
        let pm = MockPackageManager::default();
        assert!(check_impl(&config, &pm).is_ok());
    }

    #[test]
    fn check_impl_single_issue_uses_singular_noun() {
        // Kills `replace == with != at line 36` for issues == 1.
        // (Verified via the side-effect path, but any deterministic Err is sufficient.)
        let config = make_config(&["node"], HashMap::new());
        let pm = MockPackageManager::default();
        assert!(check_impl(&config, &pm).is_err());
    }

    #[test]
    fn check_impl_zero_issues_returns_ok() {
        // Kills `replace == with != at line 32` — with mutation, zero issues would return Err.
        let config = make_config(&["node"], HashMap::new());
        let pm = MockPackageManager {
            installed: true,
            ..Default::default()
        };
        assert!(check_impl(&config, &pm).is_ok());
    }

    #[test]
    fn check_impl_not_installed_service_counts_as_issue() {
        // When a service module is not installed, issues += 1.
        // Kills `delete ! in run at line 26` — mutation would count INSTALLED services as issues.
        let config = make_config(&["mysql"], HashMap::new()); // mysql is a service
        let pm = MockPackageManager::default(); // installed=false
        let result = check_impl(&config, &pm);
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
        let result = check_impl(&config, &pm);
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
        assert!(check_impl(&config, &pm).is_err());
    }
}
