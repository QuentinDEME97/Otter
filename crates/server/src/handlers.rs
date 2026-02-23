use crate::auth::AuthenticatedClient;
use crate::error::{AppError, AppResult};
use crate::security::{decode_base64_field, fingerprint_api_key, generate_api_key, hash_api_key};
use crate::state::AppState;
use crate::storage::{file_storage_path, validate_relative_file_path};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use chrono::{DateTime, Utc};
use rustsync_core::types::FileMetadata;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use std::cmp::{max, min};
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[derive(Debug, Deserialize)]
pub struct RegisterClientRequest {
    pub name: String,
    pub public_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterClientResponse {
    pub client_id: Uuid,
    pub name: String,
    pub registered_at: DateTime<Utc>,
    pub api_key: String,
}

pub async fn register_client(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterClientRequest>,
) -> AppResult<(StatusCode, Json<RegisterClientResponse>)> {
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest(
            "Client name cannot be empty".to_string(),
        ));
    }

    let public_key = decode_base64_field(&payload.public_key, "public_key")?;
    if public_key.is_empty() {
        return Err(AppError::BadRequest(
            "public_key cannot be empty".to_string(),
        ));
    }

    let client_id = Uuid::new_v4();
    let api_key = generate_api_key();
    let api_key_fingerprint = fingerprint_api_key(&api_key);
    let api_key_hash = hash_api_key(&api_key)?;
    let registered_at = Utc::now();

    sqlx::query(
        "INSERT INTO clients (id, name, public_key, api_key_fingerprint, api_key_hash, registered_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(client_id)
    .bind(name)
    .bind(public_key)
    .bind(api_key_fingerprint)
    .bind(api_key_hash)
    .bind(registered_at)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(RegisterClientResponse {
            client_id,
            name: name.to_string(),
            registered_at,
            api_key,
        }),
    ))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadFileResponse {
    pub metadata: FileMetadata,
}

#[derive(Debug, Deserialize)]
pub struct UploadFileRequest {
    pub path: String,
    pub content_base64: String,
}

#[derive(Debug, FromRow)]
struct ExistingFileRow {
    id: Uuid,
    version: i64,
}

pub async fn upload_file(
    State(state): State<Arc<AppState>>,
    client: AuthenticatedClient,
    Json(payload): Json<UploadFileRequest>,
) -> AppResult<(StatusCode, Json<UploadFileResponse>)> {
    validate_relative_file_path(&payload.path)?;

    let content = decode_base64_field(&payload.content_base64, "content_base64")?;
    let size = u64::try_from(content.len())
        .map_err(|_| AppError::Internal("File content is too large".to_string()))?;
    let checksum = hex::encode(Sha256::digest(&content));
    let now = Utc::now();
    let mut tx = state.db.begin().await?;

    let existing =
        sqlx::query_as::<_, ExistingFileRow>("SELECT id, version FROM files WHERE path = ?1")
            .bind(&payload.path)
            .fetch_optional(&mut *tx)
            .await?;

    let (file_id, version) = match existing {
        Some(existing) => {
            let next_version = existing
                .version
                .checked_add(1)
                .ok_or_else(|| AppError::Internal("File version overflow".to_string()))?;

            sqlx::query(
                "UPDATE files
                 SET owner_client_id = ?1, size = ?2, checksum = ?3, version = ?4, last_modified = ?5
                 WHERE id = ?6",
            )
            .bind(client.id)
            .bind(i64::try_from(size).map_err(|_| {
                AppError::Internal("File size cannot fit database integer type".to_string())
            })?)
            .bind(&checksum)
            .bind(next_version)
            .bind(now)
            .bind(existing.id)
            .execute(&mut *tx)
            .await?;

            (existing.id, next_version)
        }
        None => {
            let file_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO files (id, owner_client_id, path, size, checksum, version, last_modified)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(file_id)
            .bind(client.id)
            .bind(&payload.path)
            .bind(i64::try_from(size).map_err(|_| {
                AppError::Internal("File size cannot fit database integer type".to_string())
            })?)
            .bind(&checksum)
            .bind(1_i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            (file_id, 1_i64)
        }
    };

    let storage_path = file_storage_path(&state.data_dir, file_id);
    tokio::fs::write(storage_path, content).await?;

    insert_file_log(
        &mut tx,
        Some(file_id),
        client.id,
        "upload",
        Some(json!({
            "path": payload.path,
            "client_name": client.name,
            "version": version,
            "checksum": checksum
        })),
    )
    .await?;

    tx.commit().await?;

    let metadata = FileMetadata {
        id: file_id,
        path: payload.path,
        size,
        checksum,
        last_modified: now,
        version: as_u64(version, "version")?,
    };

    Ok((StatusCode::OK, Json(UploadFileResponse { metadata })))
}

#[derive(Debug, FromRow)]
struct StoredFileRow {
    id: Uuid,
    path: String,
    size: i64,
    checksum: String,
    last_modified: DateTime<Utc>,
    version: i64,
}

pub async fn list_files(
    State(state): State<Arc<AppState>>,
    _client: AuthenticatedClient,
) -> AppResult<Json<Vec<FileMetadata>>> {
    let rows = sqlx::query_as::<_, StoredFileRow>(
        "SELECT id, path, size, checksum, last_modified, version
         FROM files
         ORDER BY last_modified DESC",
    )
    .fetch_all(&state.db)
    .await?;

    let files = rows
        .into_iter()
        .map(|row| {
            Ok(FileMetadata {
                id: row.id,
                path: row.path,
                size: as_u64(row.size, "size")?,
                checksum: row.checksum,
                last_modified: row.last_modified,
                version: as_u64(row.version, "version")?,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;

    Ok(Json(files))
}

#[derive(Debug, FromRow)]
struct StoredFilePathRow {
    id: Uuid,
    path: String,
}

pub async fn download_file(
    State(state): State<Arc<AppState>>,
    _client: AuthenticatedClient,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    let file = sqlx::query_as::<_, StoredFilePathRow>("SELECT id, path FROM files WHERE id = ?1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("File not found".to_string()))?;

    let content = tokio::fs::read(file_storage_path(&state.data_dir, file.id))
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::NotFound("File content not found".to_string())
            }
            _ => AppError::Io(error),
        })?;

    let mut response = Response::new(axum::body::Body::from(content));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        "x-rustsync-file-path",
        HeaderValue::from_str(&file.path)
            .map_err(|_| AppError::Internal("Failed to encode file path header".to_string()))?,
    );

    Ok(response)
}

pub async fn delete_file(
    State(state): State<Arc<AppState>>,
    client: AuthenticatedClient,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let existing =
        sqlx::query_as::<_, StoredFilePathRow>("SELECT id, path FROM files WHERE id = ?1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("File not found".to_string()))?;

    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM files WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    insert_file_log(
        &mut tx,
        None,
        client.id,
        "delete",
        Some(json!({
            "deleted_file_id": id,
            "path": existing.path,
            "client_name": client.name,
        })),
    )
    .await?;
    tx.commit().await?;

    let storage_path = file_storage_path(&state.data_dir, existing.id);
    if let Err(error) = tokio::fs::remove_file(storage_path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        warn!(error = %error, file_id = %id, "Failed to remove file content from storage");
    }

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct LogQueryParams {
    pub limit: Option<i64>,
}

#[derive(Debug, FromRow)]
struct FileLogRow {
    id: Uuid,
    file_id: Option<Uuid>,
    client_id: Uuid,
    action: String,
    timestamp: DateTime<Utc>,
    metadata: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FileLogResponse {
    pub id: Uuid,
    pub file_id: Option<Uuid>,
    pub client_id: Uuid,
    pub action: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<Value>,
}

pub async fn list_logs(
    State(state): State<Arc<AppState>>,
    client: AuthenticatedClient,
    Query(params): Query<LogQueryParams>,
) -> AppResult<Json<Vec<FileLogResponse>>> {
    let limit = params
        .limit
        .map(|value| max(value, 1))
        .map(|value| min(value, state.max_log_entries))
        .unwrap_or(min(50, state.max_log_entries));

    let rows = sqlx::query_as::<_, FileLogRow>(
        "SELECT id, file_id, client_id, action, timestamp, metadata
         FROM file_logs
         WHERE client_id = ?1
         ORDER BY timestamp DESC
         LIMIT ?2",
    )
    .bind(client.id)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    let response = rows
        .into_iter()
        .map(|row| {
            let metadata = row
                .metadata
                .map(|raw| {
                    serde_json::from_str::<Value>(&raw).map_err(|error| {
                        AppError::Internal(format!("Failed to decode log metadata: {error}"))
                    })
                })
                .transpose()?;

            Ok(FileLogResponse {
                id: row.id,
                file_id: row.file_id,
                client_id: row.client_id,
                action: row.action,
                timestamp: row.timestamp,
                metadata,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;

    Ok(Json(response))
}

async fn insert_file_log(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    file_id: Option<Uuid>,
    client_id: Uuid,
    action: &str,
    metadata: Option<Value>,
) -> AppResult<()> {
    let metadata = metadata
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            AppError::Internal(format!("Failed to serialize log metadata: {error}"))
        })?;

    sqlx::query(
        "INSERT INTO file_logs (id, file_id, client_id, action, timestamp, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(Uuid::new_v4())
    .bind(file_id)
    .bind(client_id)
    .bind(action)
    .bind(Utc::now())
    .bind(metadata)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

fn as_u64(value: i64, field: &str) -> AppResult<u64> {
    u64::try_from(value).map_err(|_| {
        AppError::Internal(format!(
            "Database value for {field} cannot be represented as unsigned integer"
        ))
    })
}
