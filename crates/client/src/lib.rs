pub mod api;
pub mod config;
pub mod error;
pub mod sync;
pub mod watcher;

pub use config::{ClientConfig, ServerConnectionConfig, SyncConflictPolicy, VaultConfig};
pub use error::{ClientError, ClientResult};

use crate::api::ApiClient;
use crate::sync::{handle_file_event, initial_sync};
use crate::watcher::watch_vault;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

pub async fn run(config: ClientConfig) -> ClientResult<()> {
    config.validate()?;
    let api = ApiClient::new(&config.server)?;

    let health = api.health().await?;
    if health.status != "ok" {
        return Err(ClientError::InvalidConfig(format!(
            "Server health endpoint returned unexpected status '{}'",
            health.status
        )));
    }

    initial_sync(&config, &api).await?;
    info!("Initial sync completed");

    let (tx, mut rx) = mpsc::channel(512);

    for vault in config.vaults.clone() {
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            if let Err(error) = watch_vault(vault, tx_clone).await {
                error!(error = %error, "Vault watcher stopped unexpectedly");
            }
        });
    }
    drop(tx);

    while let Some(event) = rx.recv().await {
        if let Err(error) = handle_file_event(event, &api).await {
            warn!(error = %error, "Failed to handle watcher event");
        }
    }

    Ok(())
}

pub async fn run_from_default_config() -> ClientResult<()> {
    let config_path = ClientConfig::default_path()?;
    let config = if config_path.exists() {
        ClientConfig::load_from_path(&config_path)?
    } else {
        ClientConfig::load_or_create_default()?
    };
    run(config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use server::{AppState, ServerConfig, build_router};
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::net::TcpListener;

    struct TestServer {
        base_url: String,
        _temp_dir: TempDir,
    }

    async fn spawn_server() -> TestServer {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let db_path = temp_dir.path().join("server.db");
        let data_path = temp_dir.path().join("server-data");

        let server_config = ServerConfig {
            database_url: format!("sqlite://{}", db_path.display()),
            data_dir: data_path,
            ..ServerConfig::default()
        };

        let state = Arc::new(
            AppState::new(&server_config)
                .await
                .expect("failed to initialize server state"),
        );
        let app = build_router(state);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind tcp listener");
        let address = listener
            .local_addr()
            .expect("failed to get local listener address");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("test server failed unexpectedly");
        });

        TestServer {
            base_url: format!("http://{}", address),
            _temp_dir: temp_dir,
        }
    }

    fn create_client_config(
        base_url: &str,
        api_key: String,
        vault_path: &std::path::Path,
    ) -> ClientConfig {
        ClientConfig {
            server: ServerConnectionConfig {
                url: base_url.to_string(),
                api_key,
            },
            vaults: vec![VaultConfig {
                name: "Main".to_string(),
                local_path: vault_path.to_path_buf(),
                remote_id: Some("main-vault".to_string()),
            }],
            sync_conflict_policy: SyncConflictPolicy::SkipAndLogConflict,
        }
    }

    #[tokio::test]
    #[ignore = "requires local TCP networking"]
    async fn initial_sync_uploads_local_only_file() {
        let server = spawn_server().await;
        let registration = ApiClient::register_client(&server.base_url, "Uploader", b"pub-key")
            .await
            .expect("failed to register client");
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let vault_path = temp_dir.path().join("vault");
        std::fs::create_dir_all(vault_path.join("notes"))
            .expect("failed to create vault directories");
        std::fs::write(vault_path.join("notes/local.md"), b"local data")
            .expect("failed to write local file");

        let config =
            create_client_config(&server.base_url, registration.api_key.clone(), &vault_path);
        let api = ApiClient::new(&config.server).expect("failed to build API client");

        initial_sync(&config, &api)
            .await
            .expect("initial sync should succeed");

        let files = api.list_files().await.expect("failed to list files");
        assert!(
            files
                .iter()
                .any(|file| file.path == "main-vault/notes/local.md")
        );
    }

    #[tokio::test]
    #[ignore = "requires local TCP networking"]
    async fn initial_sync_downloads_remote_only_file() {
        let server = spawn_server().await;
        let registration = ApiClient::register_client(&server.base_url, "Downloader", b"pub-key")
            .await
            .expect("failed to register client");
        let api_config = ServerConnectionConfig {
            url: server.base_url.clone(),
            api_key: registration.api_key.clone(),
        };
        let api = ApiClient::new(&api_config).expect("failed to build API client");
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let vault_path = temp_dir.path().join("vault");
        std::fs::create_dir_all(&vault_path).expect("failed to create vault directory");

        let uploaded = api
            .upload_file("main-vault/notes/remote.md", b"remote data")
            .await
            .expect("failed to upload seed remote file");
        let config = create_client_config(&server.base_url, registration.api_key, &vault_path);
        let download_api = ApiClient::new(&config.server).expect("failed to build API client");

        initial_sync(&config, &download_api)
            .await
            .expect("initial sync should succeed");

        let local_path = vault_path.join("notes/remote.md");
        assert!(local_path.exists());
        let content = std::fs::read(&local_path).expect("failed to read downloaded file");
        assert_eq!(content, b"remote data");
        assert_eq!(uploaded.path, "main-vault/notes/remote.md");
    }

    #[tokio::test]
    #[ignore = "requires local TCP networking"]
    async fn initial_sync_skips_conflict_and_keeps_local_file() {
        let server = spawn_server().await;
        let registration = ApiClient::register_client(&server.base_url, "Conflict", b"pub-key")
            .await
            .expect("failed to register client");
        let config_root = TempDir::new().expect("failed to create temp dir");
        let vault_path = config_root.path().join("vault");
        std::fs::create_dir_all(vault_path.join("notes")).expect("failed to create vault path");

        let local_file = vault_path.join("notes/conflict.md");
        std::fs::write(&local_file, b"local content").expect("failed to write local conflict file");

        let config = create_client_config(&server.base_url, registration.api_key, &vault_path);
        let api = ApiClient::new(&config.server).expect("failed to build API client");
        api.upload_file("main-vault/notes/conflict.md", b"remote content")
            .await
            .expect("failed to seed remote file");

        initial_sync(&config, &api)
            .await
            .expect("initial sync should succeed");

        let local_content = std::fs::read(local_file).expect("failed to read local conflict file");
        assert_eq!(local_content, b"local content");
    }
}
