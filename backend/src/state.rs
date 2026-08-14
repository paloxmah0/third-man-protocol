use crate::error::ApiError;
use crate::AppState;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use serde::Serialize;
use sqlx::Row;

/// Authenticated user resolved from `Authorization: Bearer <session-token>`.
#[derive(Clone, Debug, Serialize)]
pub struct AuthUser {
    pub id: String,
    pub did: String,
    pub address: String,
    pub role: String,
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let app = AppState::from_ref(state);
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.trim().to_string())
            .ok_or_else(|| ApiError::Unauthorized("missing bearer token".into()))?;

        let row = sqlx::query(
            "SELECT u.id, u.did, u.address, u.role FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token = ? AND s.expires_at > ?",
        )
        .bind(&token)
        .bind(crate::db::now_iso())
        .fetch_optional(&app.pool)
        .await?
        .ok_or_else(|| ApiError::Unauthorized("invalid or expired session".into()))?;

        Ok(AuthUser {
            id: row.try_get("id")?,
            did: row.try_get("did")?,
            address: row.try_get("address")?,
            role: row.try_get("role")?,
        })
    }
}
