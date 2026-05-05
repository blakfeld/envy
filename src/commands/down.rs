use anyhow::{Context, Result};

use crate::commands::exec::run_hook;
use crate::config::EnvyConfig;
use crate::modules;
use crate::output;
use crate::package_manager;

pub fn run(profile: &str) -> Result<()> {
    let config = EnvyConfig::load_default()?;

    let project_name = config.name.as_deref().unwrap_or("project");
    output::header(&format!("envy down · {} [{}]", project_name, profile));

    if let Some(ref hook) = config.hooks.before_down {
        output::header("Hooks");
        run_hook("before_down", hook)?;
    }

    let pm = package_manager::detect()?;

    let deps = config.normalized_dependencies(profile);
    let services: Vec<_> = deps
        .iter()
        .filter_map(|dep| {
            let module = modules::get(&dep.name);
            module.is_service().then_some((dep, module))
        })
        .collect();

    if services.is_empty() {
        output::skip("no services defined");
        return Ok(());
    }

    for (dep, module) in services {
        if !module.is_running(pm.as_ref(), dep)? {
            output::skip(&format!("{} already stopped", dep.name));
            continue;
        }

        output::step(&format!("Stopping {}", dep.name));
        module
            .stop(pm.as_ref(), dep)
            .with_context(|| format!("Failed to stop {}", dep.name))?;
        output::success(&format!("{} stopped", dep.name));
    }

    if let Some(ref hook) = config.hooks.after_down {
        output::header("Hooks");
        run_hook("after_down", hook)?;
    }

    println!();
    output::success("all services stopped");

    Ok(())
}
