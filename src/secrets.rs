use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use which::which;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

/// Installs ejson via the package manager if it is not already on PATH.
pub fn ensure_available(pm: &dyn PackageManager) -> Result<()> {
    if which("ejson").is_ok() {
        return Ok(());
    }
    pm.install_package(&Dependency::simple("ejson"))
        .context("Failed to install ejson")
}

/// Decrypts an ejson file and returns all key-value pairs, excluding `_public_key`.
/// Secret values are returned in memory only and must not be printed by callers.
pub fn decrypt(file: &Path) -> Result<HashMap<String, String>> {
    if !file.exists() {
        bail!("Secrets file not found: {}", file.display());
    }

    let output = Command::new("ejson")
        .args(["decrypt", &file.to_string_lossy()])
        .output()
        .context("Failed to run ejson decrypt")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ejson decrypt failed: {}", stderr.trim());
    }

    let json = String::from_utf8(output.stdout).context("ejson output was not valid UTF-8")?;

    let raw: HashMap<String, serde_json::Value> =
        serde_json::from_str(&json).context("Failed to parse ejson decrypt output as JSON")?;

    Ok(raw
        .into_iter()
        .filter(|(k, _)| k != "_public_key")
        .map(|(k, v)| {
            let s = match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            };
            (k, s)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn decrypt_missing_file_returns_descriptive_error() {
        let path = Path::new("/nonexistent/secrets_test.ejson");
        let err = decrypt(path).unwrap_err();
        assert!(err.to_string().contains("Secrets file not found"));
    }
}
