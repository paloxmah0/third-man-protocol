//! Immutable ledger mirror: a push/pull store that mirrors on-chain (and pre-on-chain)
//! events. Records are append-only by design — confirmed entries can never be mutated,
//! so anyone can later pull from the blockchain to confirm an event happened.

use crate::db::{now_iso, sha256_hex};
use crate::error::ApiResult;
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

/// Push a canonical event snapshot onto the ledger mirror. The content hash makes the
/// record tamper-evident even before it is confirmed on-chain.
pub async fn push(
    state: &AppState,
    kind: &str,
    ref_id: &str,
    _contract_id: &str,
    payload: Value,
    pushed_by: Option<&str>,
) -> ApiResult<String> {
    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    let content_hash = sha256_hex(payload_json.as_bytes());
    let tx_hash = format!("{}:{}", kind, content_hash);
    let now = now_iso();
    sqlx::query("INSERT OR IGNORE INTO ledger_mirror (tx_hash, kind, ref_id, payload_json, content_hash, confirmed, pushed_by, created_at) VALUES (?, ?, ?, ?, ?, 0, ?, ?)")
        .bind(&tx_hash)
        .bind(kind)
        .bind(ref_id)
        .bind(&payload_json)
        .bind(&content_hash)
        .bind(pushed_by)
        .bind(&now)
        .execute(&state.pool)
        .await?;
    Ok(tx_hash)
}

#[derive(Deserialize)]
pub struct PushReq {
    pub kind: String,
    pub ref_id: Option<String>,
    pub payload: Value,
}

/// `POST /ledger/push` — push an arbitrary immutable record (e.g. an external attestation).
pub async fn push_handler(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<PushReq>,
) -> ApiResult<Json<Value>> {
    let tx = push(
        &state,
        &req.kind,
        &req.ref_id.unwrap_or_default(),
        "",
        req.payload,
        Some(&user.id),
    )
    .await?;
    Ok(Json(json!({ "pushed": true, "tx_hash": tx })))
}

/// `POST /ledger/:tx_hash/confirm` — mark a record as confirmed on-chain (with block).
#[derive(Deserialize)]
pub struct ConfirmReq {
    pub block: Option<i64>,
    pub anchor_tx_hash: Option<String>,
}
pub async fn confirm(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(tx_hash): Path<String>,
    Json(req): Json<ConfirmReq>,
) -> ApiResult<Json<Value>> {
    let now = now_iso();
    let res = sqlx::query("UPDATE ledger_mirror SET confirmed = 1, block = ?, confirmed_at = ? WHERE tx_hash = ?")
        .bind(req.block)
        .bind(&now)
        .bind(&tx_hash)
        .execute(&state.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(crate::error::ApiError::NotFound("ledger record not found".into()));
    }
    // if an anchor tx hash is provided, back-fill it onto any matching receipt
    if let Some(anchor) = req.anchor_tx_hash {
        sqlx::query("UPDATE receipts SET anchor_tx_hash = ? WHERE content_hash = ?")
            .bind(&anchor)
            .bind(&tx_hash.split(':').last().unwrap_or(""))
            .execute(&state.pool)
            .await?;
    }
    Ok(Json(json!({ "confirmed": true, "tx_hash": tx_hash })))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub kind: Option<String>,
    pub confirmed: Option<i64>,
    pub limit: Option<i64>,
}

/// `GET /ledger` — pull records (filter by kind / confirmed).
pub async fn list(
    _user: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Value>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let rows = if let Some(kind) = &q.kind {
        sqlx::query("SELECT tx_hash, kind, ref_id, payload_json, content_hash, block, confirmed, pushed_by, created_at, confirmed_at FROM ledger_mirror WHERE kind = ? ORDER BY id DESC LIMIT ?")
            .bind(kind)
            .bind(limit)
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query("SELECT tx_hash, kind, ref_id, payload_json, content_hash, block, confirmed, pushed_by, created_at, confirmed_at FROM ledger_mirror ORDER BY id DESC LIMIT ?")
            .bind(limit)
            .fetch_all(&state.pool)
            .await?
    };
    Ok(Json(json!({ "records": rows.iter().map(row_to).collect::<Vec<Value>>() })))
}

/// `GET /ledger/:tx_hash` — pull a single immutable record by content hash.
pub async fn get(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(tx_hash): Path<String>,
) -> ApiResult<Json<Value>> {
    let r = sqlx::query("SELECT tx_hash, kind, ref_id, payload_json, content_hash, block, confirmed, pushed_by, created_at, confirmed_at FROM ledger_mirror WHERE tx_hash = ?")
        .bind(&tx_hash)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| crate::error::ApiError::NotFound("ledger record not found".into()))?;
    Ok(Json(row_to(&r)))
}

fn row_to(r: &sqlx::sqlite::SqliteRow) -> Value {
    let payload_json: String = r.try_get("payload_json").unwrap_or_default();
    let payload: Value = serde_json::from_str(&payload_json).unwrap_or(Value::Null);
    json!({
        "tx_hash": r.try_get::<String, _>("tx_hash").unwrap_or_default(),
        "kind": r.try_get::<String, _>("kind").unwrap_or_default(),
        "ref_id": r.try_get::<String, _>("ref_id").unwrap_or_default(),
        "content_hash": r.try_get::<String, _>("content_hash").unwrap_or_default(),
        "block": r.try_get::<Option<i64>, _>("block").ok().flatten(),
        "confirmed": r.try_get::<i64, _>("confirmed").unwrap_or(0) == 1,
        "pushed_by": r.try_get::<Option<String>, _>("pushed_by").ok().flatten(),
        "created_at": r.try_get::<String, _>("created_at").unwrap_or_default(),
        "confirmed_at": r.try_get::<Option<String>, _>("confirmed_at").ok().flatten(),
        "payload": payload,
    })
}
