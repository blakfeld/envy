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
#[mutants::skip] // success/failure branches require a real ejson binary with a valid key pair
pub fn decrypt(file: &Path) -> Result<HashMap<String, String>> {
    if !file.exists() {
        bail!("Secrets file not found: {}", file.display());
    }

    let output = Command::new("ejson")
        .arg("decrypt")
        .arg(file)
        .output()
        .context("Failed to run ejson decrypt")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ejson decrypt failed: {}", stderr.trim());
    }

    let json = String::from_utf8(output.stdout).context("ejson output was not valid UTF-8")?;

    let raw: HashMap<String, serde_json::Value> =
        serde_json::from_str(&json).context("Failed to parse ejson decrypt output as JSON")?;

    Ok(filter_secrets(raw))
}

/// Filters out the `_public_key` field and normalises values to strings.
pub(crate) fn filter_secrets(raw: HashMap<String, serde_json::Value>) -> HashMap<String, String> {
    raw.into_iter()
        .filter(|(k, _)| k != "_public_key")
        .map(|(k, v)| {
            let s = match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            };
            (k, s)
        })
        .collect()
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

    #[test]
    fn decrypt_file_exists_but_ejson_fails_returns_error() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("envy_secrets_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.ejson");
        // Write a file that is not valid ejson — ejson decrypt will fail.
        writeln!(std::fs::File::create(&path).unwrap(), "not valid ejson").unwrap();
        // decrypt should fail (either ejson not found, or ejson rejects the file).
        let result = decrypt(&path);
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_available_succeeds_when_ejson_on_path() {
        use crate::package_manager::MockPackageManager;
        // If ejson is on PATH already, ensure_available should return Ok without installing.
        // If ejson is not on PATH, it tries to install via MockPackageManager (which succeeds).
        let pm = MockPackageManager::default();
        // Either way, with a non-failing PM the result should be Ok.
        assert!(ensure_available(&pm).is_ok());
    }

    #[test]
    fn ensure_available_propagates_install_error_when_ejson_missing() {
        use crate::package_manager::MockPackageManager;
        if which::which("ejson").is_ok() {
            return;
        }
        let pm = MockPackageManager {
            install_fails: true,
            ..Default::default()
        };
        assert!(ensure_available(&pm).is_err());
    }

    #[test]
    fn filter_secrets_removes_public_key() {
        let mut raw = HashMap::new();
        raw.insert(
            "_public_key".into(),
            serde_json::Value::String("abc123".into()),
        );
        raw.insert(
            "MY_SECRET".into(),
            serde_json::Value::String("hunter2".into()),
        );
        raw.insert("OTHER".into(), serde_json::Value::String("val".into()));

        let result = filter_secrets(raw);
        assert!(
            !result.contains_key("_public_key"),
            "_public_key must be excluded"
        );
        assert_eq!(result.get("MY_SECRET").map(|s| s.as_str()), Some("hunter2"));
        assert_eq!(result.get("OTHER").map(|s| s.as_str()), Some("val"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filter_secrets_keeps_all_non_public_keys() {
        let mut raw = HashMap::new();
        raw.insert("A".into(), serde_json::Value::String("1".into()));
        raw.insert("B".into(), serde_json::Value::String("2".into()));
        let result = filter_secrets(raw);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filter_secrets_with_only_public_key_returns_empty() {
        let mut raw = HashMap::new();
        raw.insert(
            "_public_key".into(),
            serde_json::Value::String("key".into()),
        );
        let result = filter_secrets(raw);
        assert!(
            result.is_empty(),
            "_public_key must be the only key filtered out"
        );
    }

    #[test]
    fn filter_secrets_normalises_non_string_values() {
        let mut raw = HashMap::new();
        raw.insert("NUM".into(), serde_json::Value::Number(42.into()));
        raw.insert("BOOL".into(), serde_json::Value::Bool(true));
        let result = filter_secrets(raw);
        assert_eq!(result.get("NUM").map(|s| s.as_str()), Some("42"));
        assert_eq!(result.get("BOOL").map(|s| s.as_str()), Some("true"));
    }
}
