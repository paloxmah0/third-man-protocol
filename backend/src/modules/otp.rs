//! Expiring OTP invite links. The author decides capacity (`max_uses`) and TTL.
//! Joining via the code adds the caller as a participant (and consumes a use).

use crate::db::{expires_iso, now_iso, random_uuid, short_code};
use crate::error::{ApiError, ApiResult};
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Deserialize)]
pub struct CreateOtpReq {
    pub agreement_id: String,
    pub max_uses: Option<i64>,
    pub ttl_seconds: Option<i64>,
}

#[derive(Serialize)]
pub struct OtpResp {
    pub id: String,
    pub agreement_id: String,
    pub code: String,
    pub link: String,
    pub max_uses: i64,
    pub uses: i64,
    pub expires_at: String,
}

/// `POST /otp` — create an expiring invite for an agreement (author only).
pub async fn create(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateOtpReq>,
) -> ApiResult<Json<OtpResp>> {
    let ag = sqlx::query("SELECT author_id, max_participants FROM agreements WHERE id = ?")
        .bind(&req.agreement_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("agreement not found".into()))?;
    let author_id: String = ag.try_get("author_id")?;
    let max_participants: i64 = ag.try_get("max_participants")?;
    if author_id != user.id {
        return Err(ApiError::Forbidden("only the author may issue invites".into()));
    }

    let max_uses = req
        .max_uses
        .unwrap_or(state.cfg.otp_default_max_uses)
        .min(max_participants.saturating_sub(1).max(1));
    let ttl = req.ttl_seconds.unwrap_or(state.cfg.otp_default_ttl_seconds);
    let code = short_code(8);
    let id = random_uuid();
    let now = now_iso();
    let exp = expires_iso(ttl);

    sqlx::query("INSERT INTO otp_links (id, agreement_id, code, max_uses, uses, expires_at, created_at) VALUES (?, ?, ?, ?, 0, ?, ?)")
        .bind(&id)
        .bind(&req.agreement_id)
        .bind(&code)
        .bind(max_uses)
        .bind(&exp)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    let link = format!("/invite/{}", code);
    Ok(Json(OtpResp {
        id,
        agreement_id: req.agreement_id,
        code,
        link,
        max_uses,
        uses: 0,
        expires_at: exp,
    }))
}

#[derive(Deserialize)]
pub struct RedeemQuery {
    pub code: String,
    pub role: Option<String>, // supplier | buyer (defaults buyer)
}

/// `POST /otp/redeem` — join an agreement using the OTP code. Consumes one use.
pub async fn redeem(
    user: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<RedeemQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    if user.role != "supplier" && user.role != "buyer" {
        return Err(ApiError::Forbidden("complete KYC role first".into()));
    }
    let row = sqlx::query("SELECT id, agreement_id, max_uses, uses, expires_at FROM otp_links WHERE code = ?")
        .bind(&q.code)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("invite not found".into()))?;

    let id: String = row.try_get("id")?;
    let agreement_id: String = row.try_get("agreement_id")?;
    let max_uses: i64 = row.try_get("max_uses")?;
    let uses: i64 = row.try_get("uses")?;
    let expires_at: String = row.try_get("expires_at")?;
    if now_iso() > expires_at {
        return Err(ApiError::BadRequest("invite expired".into()));
    }
    if uses >= max_uses {
        return Err(ApiError::Conflict("invite capacity reached".into()));
    }

    // capacity of the agreement itself
    let ag = sqlx::query("SELECT max_participants FROM agreements WHERE id = ?")
        .bind(&agreement_id)
        .fetch_one(&state.pool)
        .await?;
    let max_participants: i64 = ag.try_get("max_participants")?;
    let joined: i64 = sqlx::query("SELECT COUNT(*) c FROM agreement_participants WHERE agreement_id = ? AND status='joined'")
        .bind(&agreement_id)
        .fetch_one(&state.pool)
        .await?
        .try_get("c")?;
    if joined >= max_participants {
        return Err(ApiError::Conflict("agreement full".into()));
    }

    let role = q.role.unwrap_or_else(|| user.role.clone());
    let now = now_iso();
    let res = sqlx::query("INSERT OR IGNORE INTO agreement_participants (agreement_id, user_id, role, status, joined_at) VALUES (?, ?, ?, 'joined', ?)")
        .bind(&agreement_id)
        .bind(&user.id)
        .bind(&role)
        .bind(&now)
        .execute(&state.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::Conflict("already a participant".into()));
    }

    sqlx::query("UPDATE otp_links SET uses = uses + 1 WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    Ok(crate::db::json_ok(serde_json::json!({
        "joined": true,
        "agreement_id": agreement_id,
        "role": role,
    })))
}

/// `GET /agreements/:id/participants`
pub async fn participants(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT p.user_id, p.role, p.status, u.did, u.address FROM agreement_participants p JOIN users u ON u.id = p.user_id WHERE p.agreement_id = ?",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    let mut v = Vec::new();
    for r in rows {
        v.push(serde_json::json!({
            "user_id": r.try_get::<String, _>("user_id")?,
            "role": r.try_get::<String, _>("role")?,
            "status": r.try_get::<String, _>("status")?,
            "did": r.try_get::<String, _>("did")?,
            "address": r.try_get::<String, _>("address")?,
        }));
    }
    Ok(Json(serde_json::json!({ "participants": v })))
}
