use anyhow::{Result, bail};
use std::path::Path;

use crate::output;

pub fn run(force: bool, config_path: &Path) -> Result<()> {
    if config_path.exists() && !force {
        bail!("envy.yml already exists. Use {} to overwrite.", "--force");
    }

    let content = "\
name: my-project

dependencies: []

environment: {}

commands: {}
";

    std::fs::write(config_path, content)?;

    output::success(&format!("wrote {}", config_path.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_config() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "envy_init_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("envy.yml")
    }

    #[test]
    fn run_creates_config_when_none_exists() {
        let path = tmp_config();
        let result = run(false, &path);
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(result.is_ok());
        assert!(content.contains("dependencies:"));
    }

    #[test]
    fn run_fails_when_config_exists_without_force() {
        let path = tmp_config();
        std::fs::write(&path, "name: existing\n").unwrap();
        let result = run(false, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn run_force_overwrites_existing_config() {
        let path = tmp_config();
        std::fs::write(&path, "name: old\n").unwrap();
        let result = run(true, &path);
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(result.is_ok());
        assert!(!content.contains("name: old"));
    }
}
