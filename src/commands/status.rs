use anyhow::{Context, Result};

use crate::config::DevyConfig;
use crate::env_manager::{EnvManager, Shadowenv};
use crate::output;
use crate::package_manager;
use crate::package_manager::PackageManager;

use super::shared;

pub(crate) fn status_impl(
    config: &DevyConfig,
    pm: &dyn PackageManager,
    env_mgr: &dyn EnvManager,
    project_root: &std::path::Path,
) -> Result<()> {
    let project_name = config.name.as_deref().unwrap_or("project");
    output::header(&format!("devy status · {}", project_name));

    let deps = config.normalized_dependencies()?;

    if !deps.is_empty() {
        output::header("Dependencies");
        shared::print_dep_table(&deps, pm, false)?;
    }

    let path_prepends: Vec<String> = deps
        .iter()
        .flat_map(|dep| crate::modules::get(&dep.name).path_prepends(dep, project_root))
        .collect();

    if !config.environment.is_empty() || !path_prepends.is_empty() {
        output::header("Environment");
        let written_vars = env_mgr.read_vars(project_root);
        shared::print_env_table(&config.environment, written_vars, false)?;

        if !path_prepends.is_empty() {
            let written_paths = env_mgr.read_path_prepends(project_root);
            shared::print_path_table(&path_prepends, written_paths, false);
        }
    }

    println!();
    Ok(())
}

#[cfg_attr(test, mutants::skip)] // thin I/O wrapper — requires a real devy.yml and package manager
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
    status_impl(&config, pm.as_ref(), &env_mgr, &project_root)
}
