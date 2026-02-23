use crate::config::VaultConfig;
use crate::error::{ClientError, ClientResult};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Component, Path, PathBuf};
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEventKind {
    Upsert,
    Remove,
}

#[derive(Debug, Clone)]
pub struct FileEvent {
    pub kind: FileEventKind,
    pub local_path: PathBuf,
    pub remote_path: String,
}

pub async fn watch_vault(vault: VaultConfig, tx: mpsc::Sender<FileEvent>) -> ClientResult<()> {
    vault.ensure_local_dir()?;

    let (notify_tx, mut notify_rx) = mpsc::unbounded_channel::<notify::Result<notify::Event>>();
    let root = vault
        .local_path
        .canonicalize()
        .unwrap_or(vault.local_path.clone());

    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |event| {
        let _ = notify_tx.send(event);
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    while let Some(result) = notify_rx.recv().await {
        let event = result?;
        let Some(kind) = map_event_kind(&event.kind) else {
            continue;
        };

        for path in event.paths {
            let Some(relative_path) = to_relative_path(&root, &path)? else {
                continue;
            };

            if kind == FileEventKind::Upsert && !path.is_file() {
                continue;
            }

            let remote_path = vault.to_remote_path(&relative_path);
            let message = FileEvent {
                kind: kind.clone(),
                local_path: path,
                remote_path,
            };

            if tx.send(message).await.is_err() {
                return Ok(());
            }
        }
    }

    Ok(())
}

fn map_event_kind(kind: &EventKind) -> Option<FileEventKind> {
    match kind {
        EventKind::Create(_) | EventKind::Modify(_) => Some(FileEventKind::Upsert),
        EventKind::Remove(_) => Some(FileEventKind::Remove),
        _ => None,
    }
}

fn to_relative_path(root: &Path, path: &Path) -> ClientResult<Option<String>> {
    let relative = match path.strip_prefix(root) {
        Ok(relative) => relative,
        Err(_) => return Ok(None),
    };

    if relative.as_os_str().is_empty() {
        return Ok(None);
    }

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
                    "Watcher observed unsupported path component in {}",
                    path.display()
                )));
            }
        }
    }

    if output.is_empty() {
        Ok(None)
    } else {
        Ok(Some(output))
    }
}
