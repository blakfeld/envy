use anyhow::Result;

use crate::config::EnvyConfig;
use crate::output;
use crate::package_manager;

use super::shared;

#[mutants::skip] // thin I/O wrapper — requires a real envy.yml and package manager
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
        shared::print_env_table(&config.environment, false)?;
    }

    println!();
    Ok(())
}
