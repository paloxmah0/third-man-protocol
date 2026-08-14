//! Agreement tailoring — a supplier or buyer authors a contract-like agreement with
//! terms, an agreement value, and a severity `weight` that scales the collateral.
//! Collateral = base + bps(value), capped. The author decides `max_participants`.

use crate::db::{now_iso, payload_hash_hex, random_uuid};
use crate::error::{ApiError, ApiResult};
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;

#[derive(Deserialize)]
pub struct CreateAgreementReq {
    pub title: String,
    pub terms: Value,           // structured contract document (recitals, scope, milestones, obligations)
    pub weight: Option<i64>,   // 1..10 (default 1)
    pub agreement_value: Option<i64>, // lovelace (default 0)
    pub max_participants: Option<i64>, // default 2
    pub currency_asset: Option<String>,
    pub release_condition: Option<String>,  // mutual_confirm | oracle | timeout_to_dispute | hybrid_arbiter
    pub dispute_window_days: Option<i64>,
    pub arbiter_fee_percent: Option<i64>,
    pub arbiter_fee_paid_by: Option<String>,
}

#[derive(Serialize)]
pub struct Agreement {
    pub id: String,
    pub author_id: String,
    pub title: String,
    pub terms: Value,
    pub terms_hash: String,
    pub weight: i64,
    pub agreement_value: i64,
    pub collateral_amount: i64,
    pub max_participants: i64,
    pub currency_asset: Option<String>,
    pub release_condition: Option<String>,
    pub dispute_window_days: i64,
    pub arbiter_fee_percent: i64,
    pub arbiter_fee_paid_by: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// `POST /agreements`
pub async fn create(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateAgreementReq>,
) -> ApiResult<Json<Agreement>> {
    require_profile_verified(&state, &user.id).await?;

    let weight = req.weight.unwrap_or(1).clamp(1, 10);
    let agreement_value = req.agreement_value.unwrap_or(0).max(0);
    let max_participants = req.max_participants.unwrap_or(2).max(2);
    let release_condition = req.release_condition.unwrap_or_else(|| "mutual_confirm".into());
    let dispute_window_days = req.dispute_window_days.unwrap_or(7);
    let arbiter_fee_percent = req.arbiter_fee_percent.unwrap_or(0);
    let arbiter_fee_paid_by = req.arbiter_fee_paid_by.unwrap_or_else(|| "party1".into());
    let collateral = compute_collateral(&state, agreement_value, weight);

    let terms_json = serde_json::to_string(&req.terms).unwrap_or_default();
    let terms_hash = payload_hash_hex(&req.terms)?;

    let id = random_uuid();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO agreements (id, author_id, title, terms_json, terms_hash, weight, agreement_value, collateral_amount, max_participants, currency_asset, release_condition, dispute_window_days, arbiter_fee_percent, arbiter_fee_paid_by, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'draft', ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&req.title)
    .bind(&terms_json)
    .bind(&terms_hash)
    .bind(weight)
    .bind(agreement_value)
    .bind(collateral)
    .bind(max_participants)
    .bind(&req.currency_asset)
    .bind(&release_condition)
    .bind(dispute_window_days)
    .bind(arbiter_fee_percent)
    .bind(&arbiter_fee_paid_by)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    // author joins as a participant
    let role = if user.role == "supplier" { "supplier" } else { "buyer" };
    sqlx::query("INSERT INTO agreement_participants (agreement_id, user_id, role, status, joined_at) VALUES (?, ?, ?, 'joined', ?)")
        .bind(&id)
        .bind(&user.id)
        .bind(role)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    fetch(&state, &id).await
}

/// `DELETE /agreements/:id` — delete a draft (only drafts can be deleted).
pub async fn delete(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let ag = sqlx::query("SELECT author_id, status FROM agreements WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("agreement not found".into()))?;
    let author_id: String = ag.try_get("author_id")?;
    let status: String = ag.try_get("status")?;
    if author_id != user.id {
        return Err(ApiError::Forbidden("only the author may delete".into()));
    }
    if status != "draft" && status != "negotiating" {
        return Err(ApiError::Conflict("only draft agreements can be deleted".into()));
    }
    // cascade deletes participants, signatures, revisions, otp_links, collateral
    sqlx::query("DELETE FROM agreements WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}

/// `GET /agreements/:id`
pub async fn get(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Agreement>> {
    fetch(&state, &id).await
}

/// `GET /agreements/:id/revisions` — list all term revisions (negotiation history).
pub async fn list_revisions(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = sqlx::query("SELECT id, version, terms_hash, proposed_by, created_at FROM agreement_revisions WHERE agreement_id = ? ORDER BY version ASC")
        .bind(&id)
        .fetch_all(&state.pool)
        .await?;
    let v: Vec<serde_json::Value> = rows.iter().map(|r| serde_json::json!({
        "id": r.try_get::<String, _>("id").unwrap_or_default(),
        "version": r.try_get::<i64, _>("version").unwrap_or(0),
        "terms_hash": r.try_get::<String, _>("terms_hash").unwrap_or_default(),
        "proposed_by": r.try_get::<String, _>("proposed_by").unwrap_or_default(),
        "created_at": r.try_get::<String, _>("created_at").unwrap_or_default(),
    })).collect();
    Ok(Json(serde_json::json!({ "revisions": v })))
}

/// `GET /agreements` — agreements the caller participates in.
pub async fn list(
    user: AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<Agreement>>> {
    let rows = sqlx::query(
        "SELECT a.* FROM agreements a
         JOIN agreement_participants p ON p.agreement_id = a.id
         WHERE p.user_id = ? ORDER BY a.created_at DESC",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await?;

    let mut out = Vec::new();
    for r in rows {
        out.push(row_to_agreement(&r)?);
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct UpdateTermsReq {
    pub terms: Value,
    pub weight: Option<i64>,
    pub agreement_value: Option<i64>,
}
/// `PATCH /agreements/:id/terms` — ANY participant can propose a change (negotiation).
/// This resets all signatures (terms changed) and sends back for re-approval.
pub async fn update_terms(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTermsReq>,
) -> ApiResult<Json<Agreement>> {
    let is_part = sqlx::query("SELECT 1 FROM agreement_participants WHERE agreement_id = ? AND user_id = ? AND status IN ('joined','signed')")
        .bind(&id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await?;
    if is_part.is_none() {
        return Err(ApiError::Forbidden("only participants may propose changes".into()));
    }

    let ag = fetch_row(&state, &id).await?;
    let weight = req.weight.unwrap_or_else(|| ag.try_get("weight").unwrap_or(1)).clamp(1, 10);
    let agreement_value = req
        .agreement_value
        .unwrap_or_else(|| ag.try_get::<i64, _>("agreement_value").unwrap_or(0))
        .max(0);
    let collateral = compute_collateral(&state, agreement_value, weight);
    let terms_json = serde_json::to_string(&req.terms).unwrap_or_default();
    let terms_hash = payload_hash_hex(&req.terms)?;
    let now = now_iso();

    let rev_id = random_uuid();
    let version: i64 = sqlx::query("SELECT COUNT(*) as c FROM agreement_revisions WHERE agreement_id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?
        .try_get::<i64, _>("c")?;
    sqlx::query("INSERT INTO agreement_revisions (id, agreement_id, version, terms_json, terms_hash, proposed_by, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(&rev_id)
        .bind(&id)
        .bind(version + 1)
        .bind(&terms_json)
        .bind(&terms_hash)
        .bind(&user.id)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    sqlx::query("UPDATE agreements SET terms_json=?, terms_hash=?, weight=?, agreement_value=?, collateral_amount=?, status='negotiating', updated_at=? WHERE id=?")
        .bind(terms_json)
        .bind(&terms_hash)
        .bind(weight)
        .bind(agreement_value)
        .bind(collateral)
        .bind(now)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    // Invalidate all previous signatures — terms changed, both parties must re-sign
    sqlx::query("DELETE FROM agreement_signatures WHERE agreement_id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    // Reset participant status from 'signed' back to 'joined'
    sqlx::query("UPDATE agreement_participants SET status='joined' WHERE agreement_id = ? AND status='signed'")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    fetch(&state, &id).await
}

/// Collateral model: base fee + weight-scaled bps of agreement value, capped.
pub fn compute_collateral(state: &AppState, agreement_value: i64, weight: i64) -> i64 {
    let base = state.cfg.collateral_base_lovelace;
    let bps = state.cfg.collateral_bps;
    let max = state.cfg.collateral_max_lovelace;
    let scaled = (agreement_value * bps / 10_000) * weight; // weight multiplies the scaled slice
    (base + scaled).min(max).max(base)
}

/// Per spec, KYC is optional and non-blocking. We don't require a profile to forge —
/// the wallet IS the identity. If a profile exists, great; if not, the user can still
/// create agreements (Tier 0 — wallet only).
async fn require_profile_verified(_state: &AppState, _user_id: &str) -> ApiResult<()> {
    Ok(())
}

fn require_author_role(user: &AuthUser) -> ApiResult<()> {
    if user.role != "supplier" && user.role != "buyer" {
        return Err(ApiError::Forbidden("only suppliers or buyers may author agreements".into()));
    }
    Ok(())
}

async fn fetch(state: &AppState, id: &str) -> ApiResult<Json<Agreement>> {
    let r = fetch_row(state, id).await?;
    Ok(Json(row_to_agreement(&r)?))
}

async fn fetch_row(state: &AppState, id: &str) -> ApiResult<sqlx::sqlite::SqliteRow> {
    sqlx::query("SELECT * FROM agreements WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("agreement not found".into()))
}

fn row_to_agreement(r: &sqlx::sqlite::SqliteRow) -> ApiResult<Agreement> {
    let terms_json: String = r.try_get("terms_json")?;
    let terms: Value = serde_json::from_str(&terms_json).unwrap_or(Value::Null);
    Ok(Agreement {
        id: r.try_get("id")?,
        author_id: r.try_get("author_id")?,
        title: r.try_get("title")?,
        terms,
        terms_hash: r.try_get("terms_hash")?,
        weight: r.try_get("weight")?,
        agreement_value: r.try_get("agreement_value")?,
        collateral_amount: r.try_get("collateral_amount")?,
        max_participants: r.try_get("max_participants")?,
        currency_asset: r.try_get("currency_asset")?,
        release_condition: r.try_get("release_condition")?,
        dispute_window_days: r.try_get("dispute_window_days")?,
        arbiter_fee_percent: r.try_get("arbiter_fee_percent")?,
        arbiter_fee_paid_by: r.try_get("arbiter_fee_paid_by")?,
        status: r.try_get("status")?,
        created_at: r.try_get("created_at")?,
        updated_at: r.try_get("updated_at")?,
    })
}
