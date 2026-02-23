use crate::config::ServerConnectionConfig;
use crate::error::{ClientError, ClientResult};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use reqwest::{Response, StatusCode};
use rustsync_core::types::FileMetadata;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Serialize)]
struct RegisterClientRequest<'a> {
    name: &'a str,
    public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterClientResponse {
    pub client_id: Uuid,
    pub name: String,
    pub registered_at: DateTime<Utc>,
    pub api_key: String,
}

#[derive(Debug, Serialize)]
struct UploadFileRequest<'a> {
    path: &'a str,
    content_base64: String,
}

#[derive(Debug, Deserialize)]
struct UploadFileResponse {
    metadata: FileMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLogResponse {
    pub id: Uuid,
    pub file_id: Option<Uuid>,
    pub client_id: Uuid,
    pub action: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<Value>,
}

impl ApiClient {
    pub fn new(config: &ServerConnectionConfig) -> ClientResult<Self> {
        let parsed = Url::parse(&config.url)
            .map_err(|error| ClientError::InvalidServerUrl(format!("{} ({error})", config.url)))?;

        let normalized = parsed.as_str().trim_end_matches('/').to_string();
        let http = reqwest::Client::builder()
            .user_agent("rustsync-client/0.1")
            .build()?;

        Ok(Self {
            http,
            base_url: normalized,
            api_key: config.api_key.clone(),
        })
    }

    pub async fn health(&self) -> ClientResult<HealthResponse> {
        let response = self.http.get(self.endpoint("/health")).send().await?;
        parse_json_response(response).await
    }

    pub async fn register_client(
        server_url: &str,
        name: &str,
        public_key: &[u8],
    ) -> ClientResult<RegisterClientResponse> {
        let parsed = Url::parse(server_url)
            .map_err(|error| ClientError::InvalidServerUrl(format!("{server_url} ({error})")))?;
        let base_url = parsed.as_str().trim_end_matches('/').to_string();
        let request = RegisterClientRequest {
            name,
            public_key: STANDARD.encode(public_key),
        };

        let response = reqwest::Client::new()
            .post(format!("{base_url}/api/clients/register"))
            .json(&request)
            .send()
            .await?;

        parse_json_response(response).await
    }

    pub async fn list_files(&self) -> ClientResult<Vec<FileMetadata>> {
        let response = self
            .authorized(self.http.get(self.endpoint("/api/files")))
            .send()
            .await?;
        parse_json_response(response).await
    }

    pub async fn upload_file(
        &self,
        remote_path: &str,
        content: &[u8],
    ) -> ClientResult<FileMetadata> {
        let request = UploadFileRequest {
            path: remote_path,
            content_base64: STANDARD.encode(content),
        };
        let response = self
            .authorized(self.http.post(self.endpoint("/api/files")))
            .json(&request)
            .send()
            .await?;
        let payload: UploadFileResponse = parse_json_response(response).await?;
        Ok(payload.metadata)
    }

    pub async fn download_file(&self, id: Uuid) -> ClientResult<Vec<u8>> {
        let response = self
            .authorized(
                self.http
                    .get(self.endpoint(&format!("/api/files/{id}/download"))),
            )
            .send()
            .await?;
        parse_bytes_response(response).await
    }

    pub async fn delete_file(&self, id: Uuid) -> ClientResult<()> {
        let response = self
            .authorized(self.http.delete(self.endpoint(&format!("/api/files/{id}"))))
            .send()
            .await?;
        parse_empty_response(response).await
    }

    pub async fn list_logs(&self, limit: u32) -> ClientResult<Vec<FileLogResponse>> {
        let response = self
            .authorized(
                self.http
                    .get(self.endpoint(&format!("/api/logs?limit={limit}"))),
            )
            .send()
            .await?;
        parse_json_response(response).await
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request.bearer_auth(&self.api_key)
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

async fn parse_json_response<T: for<'de> Deserialize<'de>>(response: Response) -> ClientResult<T> {
    let status = response.status();
    if !status.is_success() {
        return Err(http_status_error(response).await);
    }
    Ok(response.json::<T>().await?)
}

async fn parse_bytes_response(response: Response) -> ClientResult<Vec<u8>> {
    let status = response.status();
    if !status.is_success() {
        return Err(http_status_error(response).await);
    }
    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
}

async fn parse_empty_response(response: Response) -> ClientResult<()> {
    let status = response.status();
    if status == StatusCode::NO_CONTENT || status.is_success() {
        return Ok(());
    }
    Err(http_status_error(response).await)
}

async fn http_status_error(response: Response) -> ClientError {
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "Unable to decode response body".to_string());
    ClientError::HttpStatus { status, body }
}
