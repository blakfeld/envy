use anyhow::{Context, Result};

use crate::commands::exec::run_hook;
use crate::config::DevyConfig;
use crate::modules;
use crate::output;
use crate::package_manager::{self, PackageManager};

#[cfg_attr(test, mutants::skip)] // thin I/O wrapper — requires a real devy.yml and package manager
pub fn run() -> Result<()> {
    let config = DevyConfig::load_default()?;

    let project_name = config.name.as_deref().unwrap_or("project");
    output::header(&format!("devy down · {}", project_name));

    if let Some(ref hook) = config.hooks.before_down {
        output::header("Hooks");
        run_hook("before_down", hook)?;
    }

    let pm = package_manager::detect()?;
    down_impl(&config, pm.as_ref())?;

    if let Some(ref hook) = config.hooks.after_down {
        output::header("Hooks");
        run_hook("after_down", hook)?;
    }

    println!();
    Ok(())
}

pub(crate) fn down_impl(config: &DevyConfig, pm: &dyn PackageManager) -> Result<()> {
    let deps = config.normalized_dependencies()?;
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

    // Services are stopped in declaration order (the order they appear in devy.yml).
    // Modules that need to stop before a peer (e.g. Kafka before ZooKeeper) must
    // handle that ordering themselves inside their stop() implementation.
    let mut stopped_any = false;
    for (dep, module) in services {
        if !module.is_running(pm, dep)? {
            output::skip(&format!("{} already stopped", dep.name));
            continue;
        }

        output::step(&format!("Stopping {}", dep.name));
        module
            .stop(pm, dep)
            .with_context(|| format!("Failed to stop {}", dep.name))?;
        module.wait_for_stopped(pm, dep)?;
        output::success(&format!("{} stopped", dep.name));
        stopped_any = true;
    }

    if stopped_any {
        output::success("all services stopped");
    } else {
        output::skip("nothing to stop");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DevyConfig;
    use crate::package_manager::MockPackageManager;
    use std::collections::HashMap;

    fn make_config(dep_names: &[&str]) -> DevyConfig {
        crate::test_support::make_config(dep_names, HashMap::new())
    }

    #[test]
    fn down_impl_stops_running_service() {
        // Kills `delete ! in run` — mutation would skip stop when service IS running.
        let config = make_config(&["mysql"]);
        let pm = MockPackageManager {
            service_running: true,
            ..Default::default()
        };
        down_impl(&config, &pm).unwrap();
        assert!(
            !pm.stopped_services.borrow().is_empty(),
            "stop must be called for a running service"
        );
    }

    #[test]
    fn down_impl_propagates_stop_error() {
        // Kills `replace run -> Ok(())` — mutation always returns Ok.
        let config = make_config(&["mysql"]);
        let pm = MockPackageManager {
            service_running: true,
            stop_service_fails: true,
            ..Default::default()
        };
        assert!(
            down_impl(&config, &pm).is_err(),
            "stop failure must be propagated as Err"
        );
    }

    #[test]
    fn down_impl_skips_service_already_stopped() {
        let config = make_config(&["mysql"]);
        let pm = MockPackageManager {
            service_running: false,
            ..Default::default()
        };
        down_impl(&config, &pm).unwrap();
        assert!(pm.stopped_services.borrow().is_empty());
    }

    #[test]
    fn down_impl_returns_ok_with_no_services() {
        let config = make_config(&["node"]); // node is not a service
        let pm = MockPackageManager::default();
        assert!(down_impl(&config, &pm).is_ok());
    }
}
