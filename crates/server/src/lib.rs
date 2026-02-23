mod auth;
mod config;
mod error;
mod handlers;
mod security;
mod state;
mod storage;

pub use config::ServerConfig;
pub use error::{AppError, AppResult};
pub use state::AppState;

use axum::{Router, routing::get};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route(
            "/api/clients/register",
            axum::routing::post(handlers::register_client),
        )
        .route(
            "/api/files",
            axum::routing::get(handlers::list_files).post(handlers::upload_file),
        )
        .route(
            "/api/files/:id",
            axum::routing::delete(handlers::delete_file),
        )
        .route("/api/files/:id/download", get(handlers::download_file))
        .route("/api/logs", get(handlers::list_logs))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn serve(config: ServerConfig) -> AppResult<()> {
    init_tracing();

    let state = Arc::new(AppState::new(&config).await?);
    let app = build_router(state);
    let listener = TcpListener::bind(config.socket_addr())
        .await
        .map_err(AppError::from)?;

    info!(address = %config.socket_addr(), "Server started");
    axum::serve(listener, app).await.map_err(AppError::from)
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).try_init();
}

#[cfg(test)]
mod tests;
