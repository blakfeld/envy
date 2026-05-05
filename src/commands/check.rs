use anyhow::Result;
use colored::Colorize;
use std::path::Path;

use crate::config::EnvyConfig;
use crate::env_manager::shadowenv;
use crate::output;
use crate::package_manager;

use super::shared;

pub fn run(profile: &str) -> Result<()> {
    let config = EnvyConfig::load_default()?;

    let project_name = config.name.as_deref().unwrap_or("project");
    output::header(&format!("envy check · {} [{}]", project_name, profile));

    let pm = package_manager::detect()?;
    let mut issues: usize = 0;

    let deps = config.normalized_dependencies(profile);
    if !deps.is_empty() {
        output::header("Dependencies");
        issues += shared::print_dep_table(&deps, pm.as_ref(), true)?;
    }

    if !config.environment.is_empty() {
        output::header("Environment");

        let written = shadowenv::read_vars(Path::new(shadowenv::ENV_FILE));
        let key_col = config
            .environment
            .keys()
            .map(|k| k.len())
            .max()
            .unwrap_or(0);

        match written {
            None => {
                issues += config.environment.len();
                println!(
                    "  {}  environment not configured — run {}",
                    "✗".red().bold(),
                    "envy up".bold()
                );
            }
            Some(ref vars) => {
                for key in config.environment.keys() {
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
        }
    }

    println!();
    if issues == 0 {
        output::success("all checks passed");
    } else {
        let noun = if issues == 1 { "issue" } else { "issues" };
        eprintln!("  {}  {} {} found", "✗".red().bold(), issues, noun);
        std::process::exit(1);
    }

    Ok(())
}
