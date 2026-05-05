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
    output::header(&format!("envy status · {} [{}]", project_name, profile));

    let pm = package_manager::detect()?;
    let deps = config.normalized_dependencies(profile);

    if !deps.is_empty() {
        output::header("Dependencies");
        shared::print_dep_table(&deps, pm.as_ref(), false)?;
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
            None => output::info("not configured — run envy up first"),
            Some(vars) => {
                for key in config.environment.keys() {
                    let value = vars
                        .get(key)
                        .map(String::as_str)
                        .unwrap_or("(not set)")
                        .dimmed();
                    println!("  {:<key_col$}  {}", key, value);
                }
            }
        }
    }

    println!();
    Ok(())
}
