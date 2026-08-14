//! Receipts: after a successful contract, the receipt is saved to the wallet and
//! anchored on-chain via a CIP-10 metadata transaction so it can be pulled & verified.

use crate::db::{now_iso, random_uuid};
use crate::error::ApiResult;
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};
use sqlx::Row;

/// Save a receipt record (called internally after a release/slash).
pub async fn save_receipt(
    state: &AppState,
    contract_id: &str,
    user_id: &str,
    content: Value,
) -> ApiResult<String> {
    let now = now_iso();
    let id = random_uuid();
    let content_json = serde_json::to_string(&content).unwrap_or_default();
    let content_hash = crate::db::blake2b_256_hex(content_json.as_bytes());
    // anchor_tx_hash is set when the CIP-10 metadata tx is submitted; left null here.
    sqlx::query("INSERT INTO receipts (id, contract_id, user_id, content_hash, content_json, anchor_tx_hash, saved_at) VALUES (?, ?, ?, ?, ?, NULL, ?)")
        .bind(&id)
        .bind(contract_id)
        .bind(user_id)
        .bind(&content_hash)
        .bind(&content_json)
        .bind(&now)
        .execute(&state.pool)
        .await?;
    Ok(id)
}

/// `GET /receipts` — receipts owned by the caller (saved in their wallet).
pub async fn list_mine(
    user: AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT id, contract_id, content_hash, content_json, anchor_tx_hash, saved_at FROM receipts WHERE user_id = ? ORDER BY saved_at DESC")
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await?;
    let v: Vec<Value> = rows
        .iter()
        .map(|r| {
            let content_json: String = r.try_get("content_json").unwrap_or_default();
            let content: Value = serde_json::from_str(&content_json).unwrap_or(Value::Null);
            json!({
                "id": r.try_get::<String, _>("id").unwrap_or_default(),
                "contract_id": r.try_get::<String, _>("contract_id").unwrap_or_default(),
                "content_hash": r.try_get::<String, _>("content_hash").unwrap_or_default(),
                "anchor_tx_hash": r.try_get::<Option<String>, _>("anchor_tx_hash").ok().flatten(),
                "saved_at": r.try_get::<String, _>("saved_at").unwrap_or_default(),
                "content": content,
            })
        })
        .collect();
    Ok(Json(json!({ "receipts": v })))
}

/// `GET /receipts/:id`
pub async fn get(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let r = sqlx::query("SELECT id, contract_id, content_hash, content_json, anchor_tx_hash, saved_at FROM receipts WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| crate::error::ApiError::NotFound("receipt not found".into()))?;
    let content_json: String = r.try_get("content_json")?;
    let content: Value = serde_json::from_str(&content_json).unwrap_or(Value::Null);
    Ok(Json(json!({
        "id": r.try_get::<String, _>("id")?,
        "contract_id": r.try_get::<String, _>("contract_id")?,
        "content_hash": r.try_get::<String, _>("content_hash")?,
        "anchor_tx_hash": r.try_get::<Option<String>, _>("anchor_tx_hash").ok().flatten(),
        "saved_at": r.try_get::<String, _>("saved_at")?,
        "content": content,
    })))
}
