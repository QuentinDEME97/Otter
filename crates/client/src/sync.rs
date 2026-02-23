use crate::api::ApiClient;
use crate::config::{ClientConfig, SyncConflictPolicy, VaultConfig};
use crate::error::{ClientError, ClientResult};
use crate::watcher::{FileEvent, FileEventKind};
use rustsync_core::crypto::calculate_checksum;
use rustsync_core::types::FileMetadata;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use tracing::{info, warn};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
struct LocalFileSnapshot {
    absolute_path: PathBuf,
    relative_path: String,
    checksum: String,
}

pub async fn initial_sync(config: &ClientConfig, api: &ApiClient) -> ClientResult<()> {
    config.validate()?;

    for vault in &config.vaults {
        vault.ensure_local_dir()?;
        sync_vault(vault, &config.sync_conflict_policy, api).await?;
    }

    Ok(())
}

pub async fn handle_file_event(event: FileEvent, api: &ApiClient) -> ClientResult<()> {
    match event.kind {
        FileEventKind::Upsert => {
            if !event.local_path.is_file() {
                return Ok(());
            }
            let content = tokio::fs::read(&event.local_path).await?;
            api.upload_file(&event.remote_path, &content).await?;
            info!(
                remote_path = %event.remote_path,
                local_path = %event.local_path.display(),
                "Uploaded local file update",
            );
        }
        FileEventKind::Remove => {
            let files = api.list_files().await?;
            if let Some(file) = files
                .into_iter()
                .find(|file| file.path == event.remote_path)
            {
                api.delete_file(file.id).await?;
                info!(remote_path = %event.remote_path, "Deleted remote file");
            } else {
                info!(
                    remote_path = %event.remote_path,
                    "Remote file not found during delete event, skipping",
                );
            }
        }
    }

    Ok(())
}

async fn sync_vault(
    vault: &VaultConfig,
    conflict_policy: &SyncConflictPolicy,
    api: &ApiClient,
) -> ClientResult<()> {
    let remote_files = api.list_files().await?;
    let remote_index = remote_index_for_vault(vault, remote_files);
    let local_index = scan_local_files(vault)?;

    for (relative_path, remote_file) in &remote_index {
        match local_index.get(relative_path) {
            None => {
                let content = api.download_file(remote_file.id).await?;
                let local_path = vault.local_path.join(relative_path);
                if let Some(parent) = local_path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&local_path, content).await?;
                info!(
                    remote_path = %remote_file.path,
                    local_path = %local_path.display(),
                    "Downloaded remote file during initial sync",
                );
            }
            Some(local_file) => {
                if local_file.checksum != remote_file.checksum {
                    match conflict_policy {
                        SyncConflictPolicy::SkipAndLogConflict => {
                            warn!(
                                remote_path = %remote_file.path,
                                local_path = %local_file.absolute_path.display(),
                                local_checksum = %local_file.checksum,
                                remote_checksum = %remote_file.checksum,
                                "Conflict detected during initial sync, skipping file",
                            );
                        }
                    }
                }
            }
        }
    }

    let remote_rel_paths: HashSet<&str> = remote_index.keys().map(String::as_str).collect();
    for local_file in local_index.values() {
        if !remote_rel_paths.contains(local_file.relative_path.as_str()) {
            let content = tokio::fs::read(&local_file.absolute_path).await?;
            let remote_path = vault.to_remote_path(&local_file.relative_path);
            api.upload_file(&remote_path, &content).await?;
            info!(
                remote_path = %remote_path,
                local_path = %local_file.absolute_path.display(),
                "Uploaded local-only file during initial sync",
            );
        }
    }

    Ok(())
}

fn remote_index_for_vault(
    vault: &VaultConfig,
    remote_files: Vec<FileMetadata>,
) -> HashMap<String, FileMetadata> {
    remote_files
        .into_iter()
        .filter_map(|file| {
            vault
                .from_remote_path(&file.path)
                .map(|relative| (relative, file))
        })
        .collect()
}

fn scan_local_files(vault: &VaultConfig) -> ClientResult<HashMap<String, LocalFileSnapshot>> {
    let mut files = HashMap::new();

    for entry in WalkDir::new(&vault.local_path) {
        let entry =
            entry.map_err(|error| ClientError::InvalidPath(format!("WalkDir error: {error}")))?;
        if !entry.file_type().is_file() {
            continue;
        }

        let absolute_path = entry.path().to_path_buf();
        let relative_path = relative_path_string(&vault.local_path, &absolute_path)?;
        let checksum = calculate_checksum(&absolute_path)?;

        files.insert(
            relative_path.clone(),
            LocalFileSnapshot {
                absolute_path,
                relative_path,
                checksum,
            },
        );
    }

    Ok(files)
}

fn relative_path_string(root: &Path, absolute_path: &Path) -> ClientResult<String> {
    let relative = absolute_path.strip_prefix(root).map_err(|_| {
        ClientError::InvalidPath(format!(
            "Path '{}' is outside vault root '{}'",
            absolute_path.display(),
            root.display()
        ))
    })?;

    let mut output = String::new();
    for (index, component) in relative.components().enumerate() {
        match component {
            Component::Normal(value) => {
                if index > 0 {
                    output.push('/');
                }
                output.push_str(&value.to_string_lossy());
            }
            _ => {
                return Err(ClientError::InvalidPath(format!(
                    "Relative path contains unsupported component: {}",
                    absolute_path.display()
                )));
            }
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn relative_path_uses_forward_slashes() {
        let root = PathBuf::from("/tmp/vault");
        let file = PathBuf::from("/tmp/vault/notes/today.md");
        let relative = relative_path_string(&root, &file).expect("relative path should work");
        assert_eq!(relative, "notes/today.md");
    }
}
