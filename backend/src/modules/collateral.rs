//! Collateral: a weight-scaled fee locked by each party before the escrow starts.
//! On success it's returned; on a fault / arbiter verdict against a party it is
//! slashed and paid to the counterparty.

use crate::db::{now_iso, random_uuid};
use crate::error::{ApiError, ApiResult};
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::Path;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;

#[derive(Deserialize)]
pub struct LockCollateralReq {
    pub agreement_id: String,
}

#[derive(Serialize)]
pub struct CollateralResp {
    pub id: String,
    pub agreement_id: String,
    pub user_id: String,
    pub amount: i64,
    pub status: String,
}

/// `POST /collateral/lock` — builds a real collateral lock tx via Pallas.
pub async fn lock(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<LockCollateralReq>,
) -> ApiResult<Json<Value>> {
    let ag = sqlx::query("SELECT collateral_amount FROM agreements WHERE id = ?")
        .bind(&req.agreement_id)
        .fetch_one(&state.pool)
        .await?;
    let amount: i64 = ag.try_get("collateral_amount")?;

    let is_part = sqlx::query("SELECT 1 FROM agreement_participants WHERE agreement_id = ? AND user_id = ?")
        .bind(&req.agreement_id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await?;
    if is_part.is_none() {
        return Err(ApiError::Forbidden("not a participant".into()));
    }

    let id = random_uuid();
    let now = now_iso();

    // Build the real collateral lock transaction via Pallas
    // The collateral is paid to the script address with a simple datum
    let script_address = state.cfg.escrow_validator_addr.clone();
    let tx_cbor = crate::modules::datum_cbor::build_collateral_tx(
        &crate::modules::koios::KoiosProvider::new(),
        &user.address,
        &script_address,
        amount as u64,
        &req.agreement_id,
    ).await.map_err(|e| ApiError::Internal(anyhow::anyhow!("Collateral tx build failed: {}", e)))?;

    // Store as pending — will be confirmed after the wallet signs + submits
    sqlx::query("INSERT OR REPLACE INTO collateral (id, agreement_id, user_id, amount, status, created_at, updated_at) VALUES (?, ?, ?, ?, 'pending', ?, ?)")
        .bind(&id)
        .bind(&req.agreement_id)
        .bind(&user.id)
        .bind(amount)
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    return Ok(Json(serde_json::json!({
        "id": id,
        "agreement_id": req.agreement_id,
        "user_id": user.id,
        "amount": amount,
        "status": "pending",
        "tx_cbor": tx_cbor,
        "instructions": "Sign tx_cbor with wallet.api.signTx(cbor, true), then POST to /collateral/submit"
    })));
}

/// `POST /collateral/submit` — submit a signed collateral lock tx.
#[derive(Deserialize)]
pub struct SubmitCollateralReq {
    pub collateral_id: String,
    pub witness: String,
}

pub async fn submit_collateral(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<SubmitCollateralReq>,
) -> ApiResult<Json<Value>> {
    let now = now_iso();

    // Verify the collateral record exists
    let row = sqlx::query("SELECT agreement_id, amount FROM collateral WHERE id = ? AND user_id = ? AND status = 'pending'")
        .bind(&req.collateral_id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("collateral record not found or already processed".into()))?;

    let agreement_id: String = row.try_get("agreement_id")?;
    let amount: i64 = row.try_get("amount")?;

    // Rebuild the unsigned tx to assemble with the witness
    let script_address = state.cfg.escrow_validator_addr.clone();
    let unsigned_cbor = crate::modules::datum_cbor::build_collateral_tx(
        &crate::modules::koios::KoiosProvider::new(),
        &user.address,
        &script_address,
        amount as u64,
        &agreement_id,
    ).await.map_err(|e| ApiError::Internal(anyhow::anyhow!("Collateral tx rebuild failed: {}", e)))?;

    // Assemble + submit
    let signed_cbor = crate::modules::tx_builder::assemble_signed_tx(&unsigned_cbor, &req.witness)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("Collateral assemble failed: {}", e)))?;

    let koios = crate::modules::koios::KoiosProvider::new();
    let tx_hash = koios.submit_tx(&signed_cbor).await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("Collateral Koios submit failed: {}", e)))?;

    // Update collateral status to locked
    sqlx::query("UPDATE collateral SET status='locked', updated_at=? WHERE id=?")
        .bind(&now)
        .bind(&req.collateral_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({
        "locked": true,
        "tx_hash": tx_hash,
        "amount": amount,
    })))
}

/// `GET /agreements/:id/collateral`
pub async fn list_for_agreement(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT user_id, amount, status FROM collateral WHERE agreement_id = ?")
        .bind(&id)
        .fetch_all(&state.pool)
        .await?;
    let v: Vec<Value> = rows
        .iter()
        .map(|r| json!({
            "user_id": r.try_get::<String, _>("user_id").unwrap_or_default(),
            "amount": r.try_get::<i64, _>("amount").unwrap_or(0),
            "status": r.try_get::<String, _>("status").unwrap_or_default(),
        }))
        .collect();
    Ok(Json(json!({ "collateral": v })))
}

/// Return collateral to all parties (called on a clean completion).
pub async fn return_collateral(state: &AppState, agreement_id: &str) -> ApiResult<()> {
    let now = now_iso();
    sqlx::query("UPDATE collateral SET status='returned', updated_at=? WHERE agreement_id=? AND status='locked'")
        .bind(&now)
        .bind(agreement_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// Slash a party's collateral and award it to the counterparty.
pub async fn slash(
    state: &AppState,
    agreement_id: &str,
    at_fault_user_id: &str,
    beneficiary_user_id: &str,
) -> ApiResult<i64> {
    let now = now_iso();
    let row = sqlx::query("SELECT amount FROM collateral WHERE agreement_id = ? AND user_id = ? AND status='locked'")
        .bind(agreement_id)
        .bind(at_fault_user_id)
        .fetch_optional(&state.pool)
        .await?;
    let Some(r) = row else {
        return Ok(0);
    };
    let amount: i64 = r.try_get("amount")?;

    sqlx::query("UPDATE collateral SET status='slashed', updated_at=? WHERE agreement_id=? AND user_id=?")
        .bind(&now)
        .bind(agreement_id)
        .bind(at_fault_user_id)
        .execute(&state.pool)
        .await?;

    // record the beneficiary credit as a points entry (in a real system this is an on-chain payout)
    crate::modules::ledger::push(
        state,
        "slash",
        agreement_id,
        at_fault_user_id,
        json!({ "at_fault": at_fault_user_id, "beneficiary": beneficiary_user_id, "amount": amount }),
        None,
    )
    .await?;

    Ok(amount)
}
