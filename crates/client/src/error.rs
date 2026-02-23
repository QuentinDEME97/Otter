use thiserror::Error;

pub type ClientResult<T> = Result<T, ClientError>;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("Core error: {0}")]
    Core(#[from] rustsync_core::error::CoreError),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("Invalid server URL: {0}")]
    InvalidServerUrl(String),
    #[error("Unexpected HTTP status {status}: {body}")]
    HttpStatus { status: u16, body: String },
}
