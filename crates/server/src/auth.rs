use crate::error::AppError;
use crate::security::{fingerprint_api_key, verify_api_key};
use crate::state::AppState;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use sqlx::FromRow;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuthenticatedClient {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, FromRow)]
struct ClientAuthRow {
    id: Uuid,
    name: String,
    api_key_hash: String,
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthenticatedClient
where
    S: Send + Sync,
    Arc<AppState>: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = Arc::<AppState>::from_ref(state);

        let authorization = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?;

        let authorization = authorization.to_str().map_err(|_| {
            AppError::Unauthorized("Authorization header is not valid ASCII".to_string())
        })?;

        let api_key = authorization
            .strip_prefix("Bearer ")
            .ok_or_else(|| {
                AppError::Unauthorized(
                    "Authorization header must use the Bearer scheme".to_string(),
                )
            })?
            .trim();

        if api_key.is_empty() {
            return Err(AppError::Unauthorized(
                "Bearer token cannot be empty".to_string(),
            ));
        }

        let fingerprint = fingerprint_api_key(api_key);
        let client = sqlx::query_as::<_, ClientAuthRow>(
            "SELECT id, name, api_key_hash FROM clients WHERE api_key_fingerprint = ?1",
        )
        .bind(fingerprint)
        .fetch_optional(&app_state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid API key".to_string()))?;

        if !verify_api_key(api_key, &client.api_key_hash)? {
            return Err(AppError::Unauthorized("Invalid API key".to_string()));
        }

        Ok(Self {
            id: client.id,
            name: client.name,
        })
    }
}
