//! CIP-8 pre-contract signing. Both wallets sign the agreement hash *before* the
//! on-chain smart contract is initiated — this is the binding off-chain commitment.
//! The signed payload is the canonical JSON of the agreement terms + participants.

use crate::crypto::{self, parse_cip8, verify_cip8};
use crate::db::{now_iso, payload_hash_hex, random_uuid};
use crate::error::{ApiError, ApiResult};
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::Path;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;

#[derive(Deserialize)]
pub struct SignAgreementReq {
    pub cose_sign1: String,
    pub cose_key: String,
}

#[derive(Serialize)]
pub struct SignatureResp {
    pub id: String,
    pub verified: bool,
    pub terms_hash: String,
    pub payload_hash: String,
    pub signed_at: String,
}

/// `POST /agreements/:id/sign` — submit a CIP-8 signature of the canonical agreement payload.
pub async fn sign(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SignAgreementReq>,
) -> ApiResult<Json<SignatureResp>> {
    let sig = parse_cip8(&req.cose_sign1, &req.cose_key)?;
    verify_cip8(&sig)?;

    // ensure participant
    let is_part = sqlx::query("SELECT 1 FROM agreement_participants WHERE agreement_id = ? AND user_id = ? AND status='joined'")
        .bind(&id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await?;
    if is_part.is_none() {
        return Err(ApiError::Forbidden("not a participant".into()));
    }

    // bind the signed payload to this agreement's canonical content
    let ag = sqlx::query("SELECT terms_json, terms_hash FROM agreements WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let terms_json: String = ag.try_get("terms_json")?;
    let terms_hash: String = ag.try_get("terms_hash")?;
    let terms: serde_json::Value = serde_json::from_str(&terms_json).unwrap_or(json!({}));

    let participants = sqlx::query("SELECT u.address, p.role FROM agreement_participants p JOIN users u ON u.id = p.user_id WHERE p.agreement_id = ? AND p.status='joined'")
        .bind(&id)
        .fetch_all(&state.pool)
        .await?;
    let parts: Vec<serde_json::Value> = participants
        .iter()
        .map(|r| json!({ "address": r.try_get::<String, _>("address").unwrap_or_default(), "role": r.try_get::<String, _>("role").unwrap_or_default() }))
        .collect();

    let canonical = json!({
        "agreement_id": id,
        "terms_hash": terms_hash,
        "terms": terms,
        "participants": parts,
    });
    let expected_payload = crypto::signable_payload_hex(&canonical);
    if hex::encode(&sig.payload).to_lowercase() != expected_payload.to_lowercase() {
        return Err(ApiError::SignatureFailed("signed payload does not match canonical agreement".into()));
    }
    let payload_hash = payload_hash_hex(&canonical)?;

    let id_sig = random_uuid();
    let now = now_iso();
    // upsert signature for (agreement, user, terms_hash)
    let res = sqlx::query(
        "INSERT OR REPLACE INTO agreement_signatures (id, agreement_id, user_id, terms_hash, payload_hash, cose_sign1, cose_key, verified, signed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(&id_sig)
    .bind(&id)
    .bind(&user.id)
    .bind(&terms_hash)
    .bind(&payload_hash)
    .bind(&req.cose_sign1)
    .bind(&req.cose_key)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    let _ = res;

    // mark participant as signed
    sqlx::query("UPDATE agreement_participants SET status='signed' WHERE agreement_id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;

    Ok(Json(SignatureResp {
        id: id_sig,
        verified: true,
        terms_hash,
        payload_hash,
        signed_at: now,
    }))
}

/// `GET /agreements/:id/signatures` — list verified signatures.
pub async fn list_signatures(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = sqlx::query("SELECT user_id, terms_hash, payload_hash, verified, signed_at FROM agreement_signatures WHERE agreement_id = ?")
        .bind(&id)
        .fetch_all(&state.pool)
        .await?;
    let mut v = Vec::new();
    for r in rows {
        v.push(json!({
            "user_id": r.try_get::<String, _>("user_id")?,
            "terms_hash": r.try_get::<String, _>("terms_hash")?,
            "payload_hash": r.try_get::<String, _>("payload_hash")?,
            "verified": r.try_get::<i64, _>("verified")? == 1,
            "signed_at": r.try_get::<String, _>("signed_at")?,
        }));
    }
    Ok(Json(json!({ "signatures": v })))
}

/// `GET /agreements/:id/signable` — returns the hex payload a wallet must sign.
pub async fn signable(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let ag = sqlx::query("SELECT terms_json, terms_hash FROM agreements WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let terms_json: String = ag.try_get("terms_json")?;
    let terms_hash: String = ag.try_get("terms_hash")?;
    let terms: serde_json::Value = serde_json::from_str(&terms_json).unwrap_or(json!({}));

    let participants = sqlx::query("SELECT u.address, p.role FROM agreement_participants p JOIN users u ON u.id = p.user_id WHERE p.agreement_id = ? AND p.status='joined'")
        .bind(&id)
        .fetch_all(&state.pool)
        .await?;
    let parts: Vec<serde_json::Value> = participants
        .iter()
        .map(|r| json!({ "address": r.try_get::<String, _>("address").unwrap_or_default(), "role": r.try_get::<String, _>("role").unwrap_or_default() }))
        .collect();

    let canonical = json!({
        "agreement_id": id,
        "terms_hash": terms_hash,
        "terms": terms,
        "participants": parts,
    });
    let payload_hex = crypto::signable_payload_hex(&canonical);
    Ok(Json(json!({ "payload_hex": payload_hex, "terms_hash": terms_hash })))
}
