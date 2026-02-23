use crate::config::ServerConfig;
use crate::error::{AppError, AppResult};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug)]
pub struct AppState {
    pub db: SqlitePool,
    pub data_dir: PathBuf,
    pub max_log_entries: i64,
}

impl AppState {
    pub async fn new(config: &ServerConfig) -> AppResult<Self> {
        std::fs::create_dir_all(&config.data_dir)?;

        let connect_options = SqliteConnectOptions::from_str(&config.database_url)
            .map_err(|error| {
                AppError::Internal(format!(
                    "Invalid database URL '{}': {error}",
                    config.database_url
                ))
            })?
            .create_if_missing(true)
            .foreign_keys(true);

        let db = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connect_options)
            .await?;

        sqlx::migrate!("./migrations").run(&db).await?;

        Ok(Self {
            db,
            data_dir: config.data_dir.clone(),
            max_log_entries: config.max_log_entries,
        })
    }
}
