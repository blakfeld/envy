use anyhow::Result;
use colored::Colorize;

use crate::config::Dependency;
use crate::modules;
use crate::package_manager::PackageManager;

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
