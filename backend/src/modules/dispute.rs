//! Dispute resolution. A participant raises a dispute; an arbiter from the trust-weighted
//! pool is assigned; an oracle may be pulled to aid the judgement; the arbiter returns a
//! CIP-8-signed `VerdictReached` payload that the Plutus validator consumes to slash.

use crate::crypto::{self, parse_cip8, verify_cip8};
use crate::db::{now_iso, payload_hash_hex, random_uuid};
use crate::error::{ApiError, ApiResult};
use crate::modules::collateral;
use crate::modules::points;
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;

#[derive(Deserialize)]
pub struct RaiseDisputeReq {
    pub agreement_id: String,
    pub reason: String,
}

#[derive(Serialize)]
pub struct DisputeResp {
    pub id: String,
    pub agreement_id: String,
    pub raised_by: String,
    pub state: String,
    pub arbiter_id: Option<String>,
    pub verdict: Option<String>,
    pub created_at: String,
}

/// `POST /disputes` — raise a dispute.
pub async fn raise(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<RaiseDisputeReq>,
) -> ApiResult<Json<DisputeResp>> {
    let is_part = sqlx::query("SELECT 1 FROM agreement_participants WHERE agreement_id = ? AND user_id = ?")
        .bind(&req.agreement_id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await?;
    if is_part.is_none() {
        return Err(ApiError::Forbidden("not a participant".into()));
    }

    let now = now_iso();
    let id = random_uuid();
    // assign an active arbiter with the most trust points (round-robin-ish top pick)
    let arbiter: Option<String> = sqlx::query(
        "SELECT a.user_id FROM arbiters a JOIN users u ON u.id = a.user_id
         WHERE a.active = 1 AND u.status = 'active' AND u.role = 'arbiter'
         ORDER BY a.trust_points DESC, a.cases_assigned ASC LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await?
    .map(|r| r.try_get::<String, _>("user_id").unwrap());

    sqlx::query("INSERT INTO disputes (id, agreement_id, raised_by, reason, state, arbiter_id, created_at) VALUES (?, ?, ?, ?, 'open', ?, ?)")
        .bind(&id)
        .bind(&req.agreement_id)
        .bind(&user.id)
        .bind(&req.reason)
        .bind(&arbiter)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    if let Some(aid) = &arbiter {
        sqlx::query("UPDATE arbiters SET cases_assigned = cases_assigned + 1 WHERE user_id = ?")
            .bind(aid)
            .execute(&state.pool)
            .await?;
        sqlx::query("UPDATE disputes SET state='in_review' WHERE id = ?")
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    // mark agreement disputed
    sqlx::query("UPDATE agreements SET status='disputed', updated_at=? WHERE id=?")
        .bind(&now)
        .bind(&req.agreement_id)
        .execute(&state.pool)
        .await?;

    fetch(&state, &id).await
}

/// `GET /disputes/:id`
pub async fn get(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DisputeResp>> {
    fetch(&state, &id).await
}

/// `POST /disputes/:id/oracle` — fetch external data to aid the judgement (non-fatal).
#[derive(Deserialize)]
pub struct OracleReq {
    pub source: String,
    pub query: String,
}
pub async fn pull_oracle(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<OracleReq>,
) -> ApiResult<Json<Value>> {
    let now = now_iso();
    let oid = random_uuid();
    let result = fetch_oracle_data(&state, &req.source, &req.query).await;
    let (status, result_json) = match result {
        Ok(v) => ("fulfilled", Some(serde_json::to_string(&v).unwrap_or_default())),
        Err(e) => ("failed", Some(json!({"error": e.to_string()}).to_string())),
    };

    sqlx::query("INSERT INTO oracle_requests (id, dispute_id, source, query, result_json, status, fetched_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&oid)
        .bind(&id)
        .bind(&req.source)
        .bind(&req.query)
        .bind(&result_json)
        .bind(status)
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({ "oracle_id": oid, "status": status })))
}

async fn fetch_oracle_data(_state: &AppState, source: &str, query: &str) -> ApiResult<Value> {
    // Non-fatal: in production this calls the configured ORACLE_ENDPOINT. For local dev
    // we return a deterministic mock so the flow is exercisable.
    if source == "mock" {
        return Ok(json!({ "source": source, "query": query, "data": { "delivery_confirmed": true, "evidence": "mock-proof" } }));
    }
    // Attempt a real HTTP GET only if an http client were wired; here we fail non-fatally
    // by returning a structured "unavailable" record rather than blocking the flow.
    Ok(json!({ "source": source, "query": query, "available": false, "note": "oracle endpoint not wired; configure ORACLE_ENDPOINT and add reqwest" }))
}

#[derive(Deserialize)]
pub struct VerdictReq {
    pub verdict: String, // favor_buyer | favor_supplier | split
    pub rationale: String,
    pub cose_sign1: String, // arbiter's CIP-8 signature of the verdict payload
    pub cose_key: String,
}

/// `POST /disputes/:id/verdict` — arbiter submits a CIP-8-signed verdict.
pub async fn submit_verdict(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<VerdictReq>,
) -> ApiResult<Json<DisputeResp>> {
    let dispute = sqlx::query("SELECT agreement_id, arbiter_id, state FROM disputes WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let agreement_id: String = dispute.try_get("agreement_id")?;
    let arbiter_id: Option<String> = dispute.try_get("arbiter_id")?;
    let d_state: String = dispute.try_get("state")?;
    if d_state == "resolved" || d_state == "closed" {
        return Err(ApiError::Conflict("dispute already resolved".into()));
    }
    if arbiter_id.as_deref() != Some(&user.id) {
        return Err(ApiError::Forbidden("not the assigned arbiter".into()));
    }
    if req.verdict != "favor_buyer" && req.verdict != "favor_supplier" && req.verdict != "split" {
        return Err(ApiError::BadRequest("invalid verdict".into()));
    }

    // verify arbiter's CIP-8 signature over the canonical verdict payload
    let verdict_payload = json!({
        "dispute_id": id,
        "agreement_id": agreement_id,
        "verdict": req.verdict,
        "rationale": req.rationale,
    });
    let expected = crypto::signable_payload_hex(&verdict_payload);
    let sig = parse_cip8(&req.cose_sign1, &req.cose_key)?;
    verify_cip8(&sig)?;
    if hex::encode(&sig.payload).to_lowercase() != expected.to_lowercase() {
        return Err(ApiError::SignatureFailed("verdict signature payload mismatch".into()));
    }
    let payload_hash = payload_hash_hex(&verdict_payload)?;

    let now = now_iso();
    sqlx::query("INSERT INTO arbiter_verdicts (id, dispute_id, arbiter_id, verdict, rationale, cose_sign1, cose_key, verified, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?)")
        .bind(random_uuid())
        .bind(&id)
        .bind(&user.id)
        .bind(&req.verdict)
        .bind(&req.rationale)
        .bind(&req.cose_sign1)
        .bind(&req.cose_key)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    // resolve dispute + slash the at-fault party
    let (buyer_id, supplier_id) = participant_roles(&state, &agreement_id).await?;
    let (at_fault, beneficiary) = match req.verdict.as_str() {
        "favor_buyer" => (supplier_id.clone(), buyer_id.clone()),
        "favor_supplier" => (buyer_id.clone(), supplier_id.clone()),
        "split" => (String::new(), String::new()), // no slashing on split; collateral returned
        _ => (String::new(), String::new()),
    };

    if !at_fault.is_empty() {
        collateral::slash(&state, &agreement_id, &at_fault, &beneficiary).await?;
    } else {
        collateral::return_collateral(&state, &agreement_id).await?;
    }

    sqlx::query("UPDATE disputes SET state='resolved', verdict=?, rationale=?, resolved_at=? WHERE id=?")
        .bind(&req.verdict)
        .bind(&req.rationale)
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;
    sqlx::query("UPDATE arbiters SET cases_resolved = cases_resolved + 1, trust_points = trust_points + 1 WHERE user_id = ?")
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    sqlx::query("UPDATE agreements SET status='completed', updated_at=? WHERE id=?")
        .bind(&now)
        .bind(&agreement_id)
        .execute(&state.pool)
        .await?;

    // anchor the verdict on the immutable ledger
    crate::modules::ledger::push(&state, "dispute_verdict", &agreement_id, &id, json!({
        "dispute_id": id,
        "verdict": req.verdict,
        "payload_hash": payload_hash,
        "arbiter": user.id,
    }), Some(&user.id)).await?;

    // award the arbiter governance points for resolving
    points::award(&state, &user.id, state.cfg.points_per_success, "verdict_issued", Some(&id)).await?;

    fetch(&state, &id).await
}

async fn participant_roles(state: &AppState, agreement_id: &str) -> ApiResult<(String, String)> {
    let rows = sqlx::query("SELECT user_id, role FROM agreement_participants WHERE agreement_id = ?")
        .bind(agreement_id)
        .fetch_all(&state.pool)
        .await?;
    let mut buyer = String::new();
    let mut supplier = String::new();
    for r in rows {
        let uid: String = r.try_get("user_id")?;
        let role: String = r.try_get("role")?;
        match role.as_str() {
            "buyer" => buyer = uid,
            "supplier" => supplier = uid,
            _ => {}
        }
    }
    Ok((buyer, supplier))
}

async fn fetch(state: &AppState, id: &str) -> ApiResult<Json<DisputeResp>> {
    let r = sqlx::query("SELECT id, agreement_id, raised_by, state, arbiter_id, verdict, created_at FROM disputes WHERE id = ?")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(DisputeResp {
        id: r.try_get("id")?,
        agreement_id: r.try_get("agreement_id")?,
        raised_by: r.try_get("raised_by")?,
        state: r.try_get("state")?,
        arbiter_id: r.try_get("arbiter_id")?,
        verdict: r.try_get("verdict")?,
        created_at: r.try_get("created_at")?,
    }))
}

/// `POST /arbiters/enroll` — a verified user (role arbiter) joins the arbiter pool.
pub async fn enroll_arbiter(
    user: AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Value>> {
    if user.role != "arbiter" {
        return Err(ApiError::BadRequest("user role is not arbiter".into()));
    }
    let now = now_iso();
    sqlx::query("INSERT OR IGNORE INTO arbiters (user_id, active, trust_points, cases_assigned, cases_resolved, joined_at) VALUES (?, 1, 0, 0, 0, ?)")
        .bind(&user.id)
        .bind(&now)
        .execute(&state.pool)
        .await?;
    Ok(crate::db::json_ok(json!({ "enrolled": true, "user_id": user.id })))
}

/// `GET /arbiters` — list the arbiter pool sorted by trust points.
pub async fn list_arbiters(
    _user: AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT a.user_id, a.active, a.trust_points, a.cases_assigned, a.cases_resolved, u.did FROM arbiters a JOIN users u ON u.id = a.user_id ORDER BY a.trust_points DESC")
        .fetch_all(&state.pool)
        .await?;
    let v: Vec<Value> = rows
        .iter()
        .map(|r| json!({
            "user_id": r.try_get::<String, _>("user_id").unwrap_or_default(),
            "did": r.try_get::<String, _>("did").unwrap_or_default(),
            "active": r.try_get::<i64, _>("active").unwrap_or(0) == 1,
            "trust_points": r.try_get::<i64, _>("trust_points").unwrap_or(0),
            "cases_assigned": r.try_get::<i64, _>("cases_assigned").unwrap_or(0),
            "cases_resolved": r.try_get::<i64, _>("cases_resolved").unwrap_or(0),
        }))
        .collect();
    Ok(Json(json!({ "arbiters": v })))
}
