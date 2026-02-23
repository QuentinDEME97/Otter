use crate::error::{ClientError, ClientResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncConflictPolicy {
    #[default]
    SkipAndLogConflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientConfig {
    pub server: ServerConnectionConfig,
    #[serde(default)]
    pub vaults: Vec<VaultConfig>,
    #[serde(default)]
    pub sync_conflict_policy: SyncConflictPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerConnectionConfig {
    pub url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultConfig {
    pub name: String,
    pub local_path: PathBuf,
    pub remote_id: Option<String>,
}

impl ClientConfig {
    pub fn default_path() -> ClientResult<PathBuf> {
        let config_root = dirs::config_dir().ok_or_else(|| {
            ClientError::InvalidConfig(
                "Cannot resolve system config directory for the current user".to_string(),
            )
        })?;
        Ok(config_root.join("rustsync").join("config.toml"))
    }

    pub fn load_from_path(path: &Path) -> ClientResult<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn save_to_path(&self, path: &Path) -> ClientResult<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn load_or_create_default() -> ClientResult<Self> {
        let path = Self::default_path()?;
        if path.exists() {
            return Self::load_from_path(&path);
        }

        let config = Self::sample();
        config.save_to_path(&path)?;
        Ok(config)
    }

    pub fn validate(&self) -> ClientResult<()> {
        if self.server.url.trim().is_empty() {
            return Err(ClientError::InvalidConfig(
                "server.url cannot be empty".to_string(),
            ));
        }
        if self.server.api_key.trim().is_empty() {
            return Err(ClientError::InvalidConfig(
                "server.api_key cannot be empty".to_string(),
            ));
        }
        if self.vaults.is_empty() {
            return Err(ClientError::InvalidConfig(
                "At least one vault must be configured".to_string(),
            ));
        }
        for vault in &self.vaults {
            vault.validate()?;
        }
        Ok(())
    }

    pub fn sample() -> Self {
        Self {
            server: ServerConnectionConfig {
                url: "http://127.0.0.1:8080".to_string(),
                api_key: "replace-with-server-api-key".to_string(),
            },
            vaults: vec![VaultConfig {
                name: "Default Vault".to_string(),
                local_path: PathBuf::from("./vault"),
                remote_id: Some("default-vault".to_string()),
            }],
            sync_conflict_policy: SyncConflictPolicy::SkipAndLogConflict,
        }
    }
}

impl VaultConfig {
    pub fn validate(&self) -> ClientResult<()> {
        if self.name.trim().is_empty() {
            return Err(ClientError::InvalidConfig(
                "Vault name cannot be empty".to_string(),
            ));
        }
        if self.namespace().is_empty() {
            return Err(ClientError::InvalidConfig(
                "Vault namespace cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    pub fn ensure_local_dir(&self) -> ClientResult<()> {
        std::fs::create_dir_all(&self.local_path)?;
        Ok(())
    }

    pub fn namespace(&self) -> String {
        if let Some(remote_id) = &self.remote_id {
            let trimmed = remote_id.trim();
            if !trimmed.is_empty() {
                return normalize_namespace(trimmed);
            }
        }
        normalize_namespace(&self.name)
    }

    pub fn to_remote_path(&self, relative_path: &str) -> String {
        format!(
            "{}/{}",
            self.namespace(),
            normalize_relative_path(relative_path)
        )
    }

    pub fn from_remote_path(&self, remote_path: &str) -> Option<String> {
        let prefix = format!("{}/", self.namespace());
        remote_path
            .strip_prefix(&prefix)
            .map(normalize_relative_path)
    }
}

fn normalize_namespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_dash = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }

    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "vault".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn config_roundtrip_save_load() {
        let dir = tempdir().expect("failed to create temp dir");
        let config_path = dir.path().join("config.toml");
        let config = ClientConfig::sample();

        config
            .save_to_path(&config_path)
            .expect("failed to save config");
        let loaded = ClientConfig::load_from_path(&config_path).expect("failed to load config");

        assert_eq!(loaded, config);
    }

    #[test]
    fn namespace_uses_remote_id_when_present() {
        let vault = VaultConfig {
            name: "Primary Vault".to_string(),
            local_path: PathBuf::from("./vault"),
            remote_id: Some("vault-main".to_string()),
        };
        assert_eq!(vault.namespace(), "vault-main");
    }

    #[test]
    fn remote_path_roundtrip() {
        let vault = VaultConfig {
            name: "Primary Vault".to_string(),
            local_path: PathBuf::from("./vault"),
            remote_id: Some("vault-main".to_string()),
        };
        let remote_path = vault.to_remote_path("notes/today.md");
        assert_eq!(remote_path, "vault-main/notes/today.md");
        assert_eq!(
            vault.from_remote_path(&remote_path),
            Some("notes/today.md".to_string())
        );
    }
}
