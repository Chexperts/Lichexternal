//! Persistent credential storage for Lichess authentication.
//!
//! Credentials are saved in `$XDG_CONFIG_HOME/lichexternal/credentials.toml` (or OS equivalent).
//! On Unix systems, file permissions are restricted to `0600` (read/write by owner only).

use anyhow::{Context, Result};
use etcetera::base_strategy::{choose_base_strategy, BaseStrategy};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Stored credentials structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Credentials {
    /// OAuth Bearer access token issued by Lichess.
    pub access_token: String,
    /// Lichess username associated with this token, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Authorized OAuth scopes (e.g. `engine:read engine:write`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Resolves the canonical credentials file path (`.../lichexternal/credentials.toml`).
#[must_use]
pub fn credentials_path() -> Option<PathBuf> {
    let strategy = choose_base_strategy().ok()?;
    Some(
        strategy
            .config_dir()
            .join("lichexternal")
            .join("credentials.toml"),
    )
}

/// Load saved credentials from disk if they exist.
pub fn load_credentials() -> Result<Option<Credentials>> {
    let Some(path) = credentials_path() else {
        return Ok(None);
    };

    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read credentials from '{}'", path.display()))?;

    let creds: Credentials = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse credentials file at '{}'", path.display()))?;

    Ok(Some(creds))
}

/// Save credentials to disk with secure owner-only permissions.
pub fn save_credentials(creds: &Credentials) -> Result<PathBuf> {
    let path = credentials_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory for credentials"))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory '{}'", parent.display()))?;
    }

    let serialized =
        toml::to_string_pretty(creds).context("Failed to serialize credentials to TOML format")?;

    fs::write(&path, serialized)
        .with_context(|| format!("Failed to write credentials to '{}'", path.display()))?;

    // On Unix systems, restrict permissions to 0600 (read/write only by user)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        let _ = fs::set_permissions(&path, perms);
    }

    Ok(path)
}

/// Delete credentials file from disk (logout).
pub fn delete_credentials() -> Result<bool> {
    let Some(path) = credentials_path() else {
        return Ok(false);
    };

    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove credentials at '{}'", path.display()))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_roundtrip() {
        let creds = Credentials {
            access_token: "lip_test123456789".to_string(),
            username: Some("test_user".to_string()),
            scope: Some("engine:read engine:write".to_string()),
        };

        let toml_str = toml::to_string_pretty(&creds).unwrap();
        let parsed: Credentials = toml::from_str(&toml_str).unwrap();

        assert_eq!(creds, parsed);
    }
}
