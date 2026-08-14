//! Negotiation state machine: participants propose/accept the current terms revision.
//! When all participants accept the latest terms_hash, the agreement moves to `agreed`
//! and becomes eligible for the CIP-8 pre-contract signing step.

use crate::db::now_iso;
use crate::error::{ApiError, ApiResult};
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::Path;
use axum::extract::State;
use axum::Json;
use serde::Serialize;
use sqlx::Row;

#[derive(Serialize)]
pub struct NegotiationStatus {
    pub agreement_id: String,
    pub terms_hash: String,
    pub status: String,
    pub participants: i64,
    pub accepted: i64,
}

/// `POST /agreements/:id/accept-terms` — accept the latest terms revision.
pub async fn accept_terms(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<NegotiationStatus>> {
    let now = now_iso();
    let exists = sqlx::query("SELECT 1 as one FROM agreement_participants WHERE agreement_id = ? AND user_id = ? AND status IN ('joined','signed')")
        .bind(&id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(ApiError::Forbidden("not a participant".into()));
    }

    // track acceptance via agreement_signatures is done in signing; here we just
    // count how many participants have a verified signature for the current hash.
    let ag = sqlx::query("SELECT terms_hash, status FROM agreements WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let terms_hash: String = ag.try_get("terms_hash")?;
    let status: String = ag.try_get("status")?;

    let participants: i64 = sqlx::query("SELECT COUNT(*) c FROM agreement_participants WHERE agreement_id = ? AND status IN ('joined','signed')")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?
        .try_get("c")?;

    let accepted: i64 = sqlx::query("SELECT COUNT(*) c FROM agreement_signatures WHERE agreement_id = ? AND terms_hash = ? AND verified = 1")
        .bind(&id)
        .bind(&terms_hash)
        .fetch_one(&state.pool)
        .await?
        .try_get("c")?;

    let new_status = if accepted >= participants && participants >= 2 {
        if status == "negotiating" || status == "draft" {
            sqlx::query("UPDATE agreements SET status='agreed', updated_at=? WHERE id=?")
                .bind(&now)
                .bind(&id)
                .execute(&state.pool)
                .await?;
        }
        "agreed".to_string()
    } else {
        "negotiating".to_string()
    };

    Ok(Json(NegotiationStatus {
        agreement_id: id,
        terms_hash,
        status: new_status,
        participants,
        accepted,
    }))
}

/// `GET /agreements/:id/negotiation` — view current negotiation status.
pub async fn status(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<NegotiationStatus>> {
    let ag = sqlx::query("SELECT terms_hash, status FROM agreements WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let terms_hash: String = ag.try_get("terms_hash")?;
    let status: String = ag.try_get("status")?;
    let participants: i64 = sqlx::query("SELECT COUNT(*) c FROM agreement_participants WHERE agreement_id = ? AND status IN ('joined','signed')")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?
        .try_get("c")?;
    let accepted: i64 = sqlx::query("SELECT COUNT(*) c FROM agreement_signatures WHERE agreement_id = ? AND terms_hash = ? AND verified = 1")
        .bind(&id)
        .bind(&terms_hash)
        .fetch_one(&state.pool)
        .await?
        .try_get("c")?;

    Ok(Json(NegotiationStatus {
        agreement_id: id,
        terms_hash,
        status,
        participants,
        accepted,
    }))
}

#[allow(dead_code)]
pub fn _u() { let _ = now_iso(); }
