use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Encryption error: {0}")]
    Crypto(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("File not found: {path}")]
    FileNotFound { path: String },
    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}
