use crate::build_router;
use crate::config::ServerConfig;
use crate::handlers::{RegisterClientResponse, UploadFileResponse};
use crate::security::fingerprint_api_key;
use crate::state::AppState;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

struct TestApp {
    app: axum::Router,
    state: Arc<AppState>,
    _temp_dir: TempDir,
}

async fn spawn_test_app() -> TestApp {
    let temp_dir = TempDir::new().expect("failed to create temporary directory");
    let db_path = temp_dir.path().join("test.db");
    let data_dir = temp_dir.path().join("data");

    let config = ServerConfig {
        database_url: format!("sqlite://{}", db_path.to_string_lossy()),
        data_dir,
        max_log_entries: 20,
        ..ServerConfig::default()
    };

    let state = Arc::new(
        AppState::new(&config)
            .await
            .expect("failed to initialize test state"),
    );
    let app = build_router(state.clone());

    TestApp {
        app,
        state,
        _temp_dir: temp_dir,
    }
}

async fn parse_json<T: DeserializeOwned>(response: axum::response::Response) -> T {
    let body = response
        .into_body()
        .collect()
        .await
        .expect("failed to collect response body")
        .to_bytes();
    serde_json::from_slice(&body).expect("failed to decode json response")
}

async fn register_client(app: &mut axum::Router) -> RegisterClientResponse {
    let request_body = json!({
        "name": "Test Client",
        "public_key": URL_SAFE_NO_PAD.encode("public-key-bytes")
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/clients/register")
                .header("content-type", "application/json")
                .body(Body::from(request_body.to_string()))
                .expect("failed to build register request"),
        )
        .await
        .expect("register request failed");

    assert_eq!(response.status(), StatusCode::CREATED);
    parse_json(response).await
}

#[tokio::test]
async fn register_client_stores_hashed_api_key() {
    let mut test_app = spawn_test_app().await;
    let registration = register_client(&mut test_app.app).await;

    let row: (String, String) =
        sqlx::query_as("SELECT api_key_fingerprint, api_key_hash FROM clients WHERE id = ?1")
            .bind(registration.client_id)
            .fetch_one(&test_app.state.db)
            .await
            .expect("failed to fetch client row");

    assert_eq!(row.0, fingerprint_api_key(&registration.api_key));
    assert_ne!(row.1, registration.api_key);
    assert!(!row.1.contains(&registration.api_key));
}

#[tokio::test]
async fn upload_download_and_delete_file() {
    let mut test_app = spawn_test_app().await;
    let registration = register_client(&mut test_app.app).await;

    let upload_body = json!({
        "path": "notes/todo.md",
        "content_base64": STANDARD.encode("hello rustsync")
    });

    let upload_response = test_app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/files")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", registration.api_key))
                .body(Body::from(upload_body.to_string()))
                .expect("failed to build upload request"),
        )
        .await
        .expect("upload request failed");
    assert_eq!(upload_response.status(), StatusCode::OK);

    let upload_json: UploadFileResponse = parse_json(upload_response).await;
    let file_id = upload_json.metadata.id;
    assert_eq!(upload_json.metadata.path, "notes/todo.md");
    assert_eq!(upload_json.metadata.version, 1);

    let list_response = test_app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/files")
                .header("authorization", format!("Bearer {}", registration.api_key))
                .body(Body::empty())
                .expect("failed to build file list request"),
        )
        .await
        .expect("list request failed");
    assert_eq!(list_response.status(), StatusCode::OK);

    let files: Vec<rustsync_core::types::FileMetadata> = parse_json(list_response).await;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].id, file_id);

    let download_response = test_app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/files/{file_id}/download"))
                .header("authorization", format!("Bearer {}", registration.api_key))
                .body(Body::empty())
                .expect("failed to build download request"),
        )
        .await
        .expect("download request failed");
    assert_eq!(download_response.status(), StatusCode::OK);
    let downloaded_content = download_response
        .into_body()
        .collect()
        .await
        .expect("failed to read download body")
        .to_bytes();
    assert_eq!(downloaded_content.as_ref(), b"hello rustsync");

    let delete_response = test_app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/files/{file_id}"))
                .header("authorization", format!("Bearer {}", registration.api_key))
                .body(Body::empty())
                .expect("failed to build delete request"),
        )
        .await
        .expect("delete request failed");
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let list_response = test_app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/files")
                .header("authorization", format!("Bearer {}", registration.api_key))
                .body(Body::empty())
                .expect("failed to build file list request"),
        )
        .await
        .expect("list request failed");
    assert_eq!(list_response.status(), StatusCode::OK);
    let files: Vec<Value> = parse_json(list_response).await;
    assert!(files.is_empty());
}

#[tokio::test]
async fn list_files_requires_authorization() {
    let test_app = spawn_test_app().await;
    let response = test_app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/files")
                .body(Body::empty())
                .expect("failed to build unauthorized list request"),
        )
        .await
        .expect("request failed");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invalid_path_is_rejected() {
    let mut test_app = spawn_test_app().await;
    let registration = register_client(&mut test_app.app).await;

    let upload_body = json!({
        "path": "../escape.txt",
        "content_base64": STANDARD.encode("hello")
    });

    let response = test_app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/files")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", registration.api_key))
                .body(Body::from(upload_body.to_string()))
                .expect("failed to build upload request"),
        )
        .await
        .expect("upload request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn logs_endpoint_returns_client_logs() {
    let mut test_app = spawn_test_app().await;
    let registration = register_client(&mut test_app.app).await;

    let upload_body = json!({
        "path": "notes/log.md",
        "content_base64": STANDARD.encode("log entry")
    });
    let _ = test_app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/files")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", registration.api_key))
                .body(Body::from(upload_body.to_string()))
                .expect("failed to build upload request"),
        )
        .await
        .expect("upload request failed");

    let response = test_app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/logs?limit=10")
                .header("authorization", format!("Bearer {}", registration.api_key))
                .body(Body::empty())
                .expect("failed to build logs request"),
        )
        .await
        .expect("logs request failed");

    assert_eq!(response.status(), StatusCode::OK);
    let logs: Vec<Value> = parse_json(response).await;
    assert!(!logs.is_empty());
    assert_eq!(logs[0]["action"], "upload");
}
