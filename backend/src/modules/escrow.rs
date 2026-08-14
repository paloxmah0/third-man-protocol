//! Smart-contract orchestration. After both wallets CIP-8-sign the agreement and
//! both lock collateral, a Plutus escrow validator is initiated. Funds are released
//! on completion, or slashed to the counterparty on a fault / arbiter verdict.
//!
//! NOTE: real Cardano tx building (CBOR via `pallas` / `cardano-serialization-lib`)
//! is behind the `TxBuilder` trait. The default `StubTxBuilder` returns structured
//! JSON so the gateway is fully exercisable locally; swap in a real builder for testnet.

use crate::db::{now_iso, random_uuid};
use crate::error::{ApiError, ApiResult};
use crate::modules::collateral;
use crate::modules::points;
use crate::modules::receipts;
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::Path;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;

/// Abstraction over building & submitting Cardano transactions for the escrow.
pub trait TxBuilder: Send + Sync {
    /// Build the lock tx (buyer locks funds + both lock collateral into the validator).
    fn build_lock_tx(&self, ctx: &TxCtx) -> ApiResult<TxDraft>;
    /// Build the release tx (validator pays out on completion).
    fn build_release_tx(&self, ctx: &TxCtx) -> ApiResult<TxDraft>;
    /// Build the slash tx (validator pays the counterparty on a fault/verdict).
    fn build_slash_tx(&self, ctx: &TxCtx) -> ApiResult<TxDraft>;
    /// Submit a signed transaction; returns the on-chain tx hash.
    fn submit(&self, tx_cbor: &str, witness: &Value) -> ApiResult<String>;
}

#[derive(Clone, Debug)]
pub struct TxCtx {
    pub smart_contract_id: String,
    pub agreement_id: String,
    pub validator_addr: String,
    pub validator_hash: String,
    pub locked_amount: i64,
    pub collateral_amount: i64,
    pub recipient: String,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TxDraft {
    pub kind: String,
    pub tx_cbor: String,
    pub unsigned_body: Value,
    pub witness_slots: Vec<String>, // user ids whose CIP-30 partial signatures are needed
}

/// Default implementation: structured JSON "tx" describing the intended action.
/// Replace with a real builder before going to testnet.
pub struct StubTxBuilder;

impl TxBuilder for StubTxBuilder {
    fn build_lock_tx(&self, ctx: &TxCtx) -> ApiResult<TxDraft> {
        Ok(TxDraft {
            kind: "lock".into(),
            tx_cbor: hex::encode(format!("STUB_LOCK:{}`{}`{}", ctx.agreement_id, ctx.locked_amount, ctx.collateral_amount).as_bytes()),
            unsigned_body: json!({
                "agreement_id": ctx.agreement_id,
                "validator_addr": ctx.validator_addr,
                "locked_amount": ctx.locked_amount,
                "collateral_amount": ctx.collateral_amount,
                "currency": "lovelace",
            }),
            witness_slots: vec![], // filled by caller with participant ids
        })
    }
    fn build_release_tx(&self, ctx: &TxCtx) -> ApiResult<TxDraft> {
        Ok(TxDraft {
            kind: "release".into(),
            tx_cbor: hex::encode(format!("STUB_RELEASE:{}`{}", ctx.agreement_id, ctx.locked_amount).as_bytes()),
            unsigned_body: json!({ "recipient": ctx.recipient, "amount": ctx.locked_amount }),
            witness_slots: vec![],
        })
    }
    fn build_slash_tx(&self, ctx: &TxCtx) -> ApiResult<TxDraft> {
        Ok(TxDraft {
            kind: "slash".into(),
            tx_cbor: hex::encode(format!("STUB_SLASH:{}`{}", ctx.agreement_id, ctx.collateral_amount).as_bytes()),
            unsigned_body: json!({ "recipient": ctx.recipient, "amount": ctx.collateral_amount, "reason": ctx.kind }),
            witness_slots: vec![],
        })
    }
    fn submit(&self, _tx_cbor: &str, _witness: &Value) -> ApiResult<String> {
        // In production this calls the connected wallet's `api.submitTx` / a node submit.
        Ok(hex::encode(&random_uuid().into_bytes()))
    }
}

#[derive(Deserialize)]
pub struct InitEscrowReq {
    pub agreement_id: String,
}

#[derive(Serialize)]
pub struct SmartContractResp {
    pub id: String,
    pub agreement_id: String,
    pub validator_hash: String,
    pub validator_addr: String,
    pub datum_hash: Option<String>,
    pub state: String,
    pub funded_so_far: i64,
    pub total_required: i64,
    pub funding_deadline: Option<String>,
}

/// `POST /escrow/init` — initiate the Plutus escrow after both parties signed + collateral locked.
pub async fn init(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<InitEscrowReq>,
) -> ApiResult<Json<SmartContractResp>> {
    let ag = sqlx::query("SELECT id, author_id, status, terms_hash, collateral_amount, agreement_value, terms_json, release_condition, dispute_window_days FROM agreements WHERE id = ?")
        .bind(&req.agreement_id)
        .fetch_one(&state.pool)
        .await?;
    let author_id: String = ag.try_get("author_id")?;
    let status: String = ag.try_get("status")?;
    if status != "agreed" {
        return Err(ApiError::Conflict("agreement must be in 'agreed' state (both wallets signed)".into()));
    }

    // any participant can deposit, but only the AUTHOR initiates the escrow
    if author_id != user.id {
        return Err(ApiError::Forbidden("only the agreement author may initiate the escrow".into()));
    }

    // require both participants to have a verified signature for the current terms_hash
    let terms_hash: String = ag.try_get("terms_hash")?;
    let accepted: i64 = sqlx::query("SELECT COUNT(*) c FROM agreement_signatures WHERE agreement_id = ? AND terms_hash = ? AND verified = 1")
        .bind(&req.agreement_id)
        .bind(&terms_hash)
        .fetch_one(&state.pool)
        .await?
        .try_get("c")?;
    let participants: i64 = sqlx::query("SELECT COUNT(*) c FROM agreement_participants WHERE agreement_id = ? AND status IN ('joined','signed')")
        .bind(&req.agreement_id)
        .fetch_one(&state.pool)
        .await?
        .try_get("c")?;
    if accepted < 2 || participants < 2 {
        return Err(ApiError::Conflict("both wallets must CIP-8-sign the agreement first".into()));
    }

    // require collateral locked by both
    let locked: i64 = sqlx::query("SELECT COUNT(*) c FROM collateral WHERE agreement_id = ? AND status='locked'")
        .bind(&req.agreement_id)
        .fetch_one(&state.pool)
        .await?
        .try_get("c")?;
    if locked < 2 {
        return Err(ApiError::Conflict("both parties must lock collateral first".into()));
    }

    let id = random_uuid();
    let now = now_iso();
    let validator_hash = state.cfg.escrow_validator_script_hash.clone();
    let validator_addr = state.cfg.escrow_validator_addr.clone();
    let datum_hash = Some(crate::db::blake2b_256_hex(req.agreement_id.as_bytes()));

    // funding deadline stored as ISO string for the DB column, but the datum uses POSIX timestamp
    let funding_deadline_iso = Some((chrono::Utc::now() + chrono::Duration::hours(24)).to_rfc3339());

    // Build the DealDatum per the new smart contract spec — with release_units
    let agreement_value: i64 = ag.try_get("agreement_value")?;
    let terms_json: String = ag.try_get("terms_json")?;
    let release_condition: String = ag.try_get("release_condition")?;
    let dispute_window_days: i64 = ag.try_get("dispute_window_days")?;

    // funding deadline: 24h from now (POSIX timestamp seconds)
    let funding_deadline_ts = (chrono::Utc::now() + chrono::Duration::hours(24)).timestamp();
    let created_at_ts = chrono::Utc::now().timestamp();

    // Fetch parties with their addresses (new schema: address + label, no role/collateral)
    let party_rows = sqlx::query("SELECT u.address, p.role FROM agreement_participants p JOIN users u ON u.id = p.user_id WHERE p.agreement_id = ? AND p.status IN ('joined','signed')")
        .bind(&req.agreement_id)
        .fetch_all(&state.pool)
        .await?;
    let parties: Vec<serde_json::Value> = party_rows.iter().enumerate().map(|(i, r)| {
        let addr: String = r.try_get("address").unwrap_or_default();
        let role: String = r.try_get("role").unwrap_or_default();
        json!({ "address": hex::encode(addr.as_bytes()), "label": hex::encode(format!("party_{}", i+1).as_bytes()) })
    }).collect();

    // Build release_units from the agreement's milestones
    // Each milestone becomes a ReleaseUnit with allocation + condition + proof
    let terms: serde_json::Value = serde_json::from_str(&terms_json).unwrap_or(json!({}));
    let milestones = terms.get("milestones").and_then(|m| m.as_array()).cloned().unwrap_or_default();
    
    let release_units: Vec<serde_json::Value> = milestones.iter().enumerate().map(|(i, m)| {
        let label = m.get("label").and_then(|v| v.as_str()).unwrap_or("milestone");
        let percent = m.get("percent").and_then(|v| v.as_i64()).unwrap_or(100);
        let amount = agreement_value * percent / 100;
        let proof_required = m.get("proofRequired").and_then(|v| v.as_bool()).unwrap_or(false);
        
        // First party is the recipient (supplier delivers, buyer receives)
        // For now, recipient is the author (buyer pays, supplier receives)
        let recipient_idx = 1; // party_2 (supplier) receives
        let recipient = parties.get(recipient_idx).and_then(|p| p.get("address")).and_then(|a| a.as_str()).unwrap_or("");
        
        json!({
            "unit_id": hex::encode(format!("unit_{}", i).as_bytes()),
            "allocation": {
                "recipient": recipient,
                "amount": amount,
            },
            "condition": if proof_required {
                json!({ "ProofRequired": {} })
            } else {
                json!({ "NoCondition": {} })
            },
            "proof": {
                "required": proof_required,
                "attachment_hash": "",
                "submitted_by": "",
                "rejection_count": 0,
                "max_attempts": 3,
                "accepted": false,
            },
            "claimed": false,
        })
    }).collect();

    // Build release_condition as a Constr-style enum matching the Aiken type
    let release_condition_datum = match release_condition.as_str() {
        "mutual_confirm" => json!({ "MutualConfirm": {} }),
        "oracle" => json!({ "OracleConfirm": { "oracle_pubkey": "" } }),
        "timeout_to_dispute" => json!({ "TimeoutDispute": { "timeout": dispute_window_days * 86400 } }),
        "hybrid_arbiter" => json!({ "HybridArbiter": { "arbiter_pubkey": "", "fee_bps": 0 } }),
        _ => json!({ "MutualConfirm": {} }),
    };

    // Build DealDatum matching the new Aiken struct (with release_units)
    let deal_datum = json!({
        "deal_id": hex::encode(id.as_bytes()),
        "parties": parties,
        "total_value": agreement_value,
        "release_units": release_units,
        "release_condition": release_condition_datum,
        "document_hash": terms_hash,
        "attachment_hashes": [],
        "dispute_window": dispute_window_days * 86400,  // convert days to seconds
        "funding_deadline": funding_deadline_ts,
        "funded_so_far": 0,
        "status": 0,  // PendingFunding
        "created_at": created_at_ts,
    });
    let deal_datum_str = serde_json::to_string(&deal_datum).unwrap_or_default();

    sqlx::query("INSERT INTO smart_contracts (id, agreement_id, validator_hash, validator_addr, datum_hash, state, funded_so_far, total_required, funding_deadline, deal_datum_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 'pending_funding', 0, ?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(&req.agreement_id)
        .bind(&validator_hash)
        .bind(&validator_addr)
        .bind(&datum_hash)
        .bind(agreement_value)
        .bind(&funding_deadline_iso)
        .bind(&deal_datum_str)
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    // Don't flip to 'active' yet — wait for the lock tx to be submitted (funding)
    sqlx::query("UPDATE agreements SET status='locked', updated_at=? WHERE id=?")
        .bind(&now)
        .bind(&req.agreement_id)
        .execute(&state.pool)
        .await?;

    // anchor the lock intent on the immutable ledger mirror
    crate::modules::ledger::push(&state, "lock_intent", &req.agreement_id, &id, json!({
        "smart_contract_id": id,
        "agreement_id": req.agreement_id,
        "validator_hash": validator_hash,
        "terms_hash": terms_hash,
        "deal_datum": deal_datum,
        "total_required": agreement_value,
        "funding_deadline": funding_deadline_iso,
    }), Some(&user.id)).await?;

    fetch(&state, &id).await
}

/// `GET /escrow/:id/lock-tx` — returns the DealDatum + unsigned tx body for the depositor
/// to sign with CIP-30 `signTx(partialSign=true)`.
pub async fn build_lock_tx(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let sc = sqlx::query("SELECT agreement_id, state, deal_datum_json, total_required, funded_so_far, validator_addr, validator_hash FROM smart_contracts WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let sc_state: String = sc.try_get("state")?;
    if sc_state != "pending_funding" {
        return Err(ApiError::Conflict("escrow not in pending_funding state".into()));
    }
    let deal_datum_str: String = sc.try_get("deal_datum_json")?;
    let deal_datum: Value = serde_json::from_str(&deal_datum_str).unwrap_or(json!({}));
    let total_required: i64 = sc.try_get("total_required")?;
    let funded_so_far: i64 = sc.try_get("funded_so_far")?;
    let remaining = total_required - funded_so_far;
    let validator_addr: String = sc.try_get("validator_addr")?;
    let agreement_id: String = sc.try_get("agreement_id")?;

    // Build proper PlutusData CBOR from the DealDatum JSON
    let datum_cbor_hex = crate::modules::datum_cbor::deal_datum_to_plutus_cbor(&deal_datum)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("DealDatum → PlutusData conversion failed: {}", e)))?;

    // Build real unsigned tx via Pallas + Koios — NO STUB FALLBACK
    let tx_cbor = crate::modules::tx_builder::build_lock_tx(
        &crate::modules::koios::KoiosProvider::new(),
        &user.address,
        &validator_addr,
        remaining as u64,
        &datum_cbor_hex,
    ).await.map_err(|e| ApiError::Internal(anyhow::anyhow!("Pallas build_lock_tx failed: {}", e)))?;

    // Record a pending funding contribution
    let contribution_id = random_uuid();
    let now = now_iso();
    sqlx::query("INSERT INTO funding_contributions (id, smart_contract_id, user_id, amount, status, created_at) VALUES (?, ?, ?, ?, 'pending', ?)")
        .bind(&contribution_id)
        .bind(&id)
        .bind(&user.id)
        .bind(remaining)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({
        "smart_contract_id": id,
        "contribution_id": contribution_id,
        "deal_datum": deal_datum,
        "remaining_to_deposit": remaining,
        "total_required": total_required,
        "funded_so_far": funded_so_far,
        "unsigned_tx": {
            "tx_cbor": tx_cbor,
            "validator_addr": validator_addr,
        },
        "instructions": "Sign this with your wallet's CIP-30 signTx(partialSign=true), then POST the witness to /escrow/:id/submit-lock-tx",
    })))
}

/// `POST /escrow/:id/submit-lock-tx` — accepts the CIP-30 witness, records it,
/// and if fully funded, flips the escrow to 'locked' (Active).
#[derive(Deserialize)]
pub struct SubmitLockReq {
    pub contribution_id: String,
    pub witness: String,  // hex CBOR transaction_witness_set
}
pub async fn submit_lock_tx(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SubmitLockReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let now = now_iso();

    // verify contribution belongs to this user + smart contract
    let contrib = sqlx::query("SELECT amount, status FROM funding_contributions WHERE id = ? AND smart_contract_id = ? AND user_id = ?")
        .bind(&req.contribution_id)
        .bind(&id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("contribution not found".into()))?;
    let amount: i64 = contrib.try_get("amount")?;
    let contrib_status: String = contrib.try_get("status")?;
    if contrib_status != "pending" {
        return Err(ApiError::Conflict("contribution already processed".into()));
    }

    // Assemble the signed transaction from the unsigned body + wallet witness
    // and submit it to the Cardano Preprod network via Koios
    let tx_hash = {
        // Assemble the signed tx from the unsigned body + wallet witness, then submit via Koios
        // We need to rebuild the unsigned tx to get the body bytes for assembly.
        // Fetch the DealDatum + rebuild:
        let sc_row = sqlx::query("SELECT deal_datum_json, total_required, funded_so_far, validator_addr FROM smart_contracts WHERE id = ?")
            .bind(&id)
            .fetch_one(&state.pool)
            .await?;
        let dd_str: String = sc_row.try_get("deal_datum_json")?;
        let dd_val: Value = serde_json::from_str(&dd_str).unwrap_or(json!({}));
        let total_req: i64 = sc_row.try_get("total_required")?;
        let funded: i64 = sc_row.try_get("funded_so_far")?;
        let remaining = total_req - funded;
        let v_addr: String = sc_row.try_get("validator_addr")?;
        let datum_hex = crate::modules::datum_cbor::deal_datum_to_plutus_cbor(&dd_val)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("DealDatum → PlutusData rebuild failed: {}", e)))?;

        // Rebuild the unsigned tx
        let unsigned_cbor = crate::modules::tx_builder::build_lock_tx(
            &crate::modules::koios::KoiosProvider::new(),
            &user.address,
            &v_addr,
            remaining as u64,
            &datum_hex,
        ).await.map_err(|e| ApiError::Internal(anyhow::anyhow!("Rebuild unsigned tx failed: {}", e)))?;

        // Assemble signed tx from unsigned body + wallet witness
        let signed_cbor = crate::modules::tx_builder::assemble_signed_tx(&unsigned_cbor, &req.witness)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("Assemble signed tx failed: {}", e)))?;

        // Submit to Cardano Preprod via Koios
        let koios = crate::modules::koios::KoiosProvider::new();
        koios.submit_tx(&signed_cbor).await
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("Koios submit_tx failed: {}", e)))?
    };

    // mark contribution as confirmed
    sqlx::query("UPDATE funding_contributions SET status='confirmed', tx_hash=?, witness=?, confirmed_at=? WHERE id=?")
        .bind(&tx_hash)
        .bind(&req.witness)
        .bind(&now)
        .bind(&req.contribution_id)
        .execute(&state.pool)
        .await?;

    // increment funded_so_far
    sqlx::query("UPDATE smart_contracts SET funded_so_far = funded_so_far + ?, updated_at=? WHERE id=?")
        .bind(amount)
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    // check if fully funded
    let sc = sqlx::query("SELECT funded_so_far, total_required, agreement_id FROM smart_contracts WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let funded_so_far: i64 = sc.try_get("funded_so_far")?;
    let total_required: i64 = sc.try_get("total_required")?;
    let agreement_id: String = sc.try_get("agreement_id")?;

    let fully_funded = funded_so_far >= total_required;
    if fully_funded {
        sqlx::query("UPDATE smart_contracts SET state='locked', updated_at=? WHERE id=?")
            .bind(&now)
            .bind(&id)
            .execute(&state.pool)
            .await?;
        sqlx::query("UPDATE agreements SET status='active', updated_at=? WHERE id=?")
            .bind(&now)
            .bind(&agreement_id)
            .execute(&state.pool)
            .await?;

        // anchor the lock confirmation
        crate::modules::ledger::push(&state, "lock_confirmed", &agreement_id, &id, json!({
            "smart_contract_id": id,
            "tx_hash": tx_hash,
            "funded_so_far": funded_so_far,
            "total_required": total_required,
            "status": "Active",
        }), Some(&user.id)).await?;
    }

    Ok(Json(json!({
        "submitted": true,
        "tx_hash": tx_hash,
        "funded_so_far": funded_so_far,
        "total_required": total_required,
        "fully_funded": fully_funded,
        "status": if fully_funded { "Escrow Locked — Active" } else { "Partially Funded" },
        "message": if fully_funded {
            "Escrow locked! The deal is now Active. Funds are held by the Plutus validator.".to_string()
        } else {
            format!("Partially funded — {} of {} ₳ received.", funded_so_far / 1_000_000, total_required / 1_000_000)
        }
    })))
}

/// `POST /escrow/:id/complete` — mark work complete; builds the release tx.
pub async fn complete(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let sc = sqlx::query("SELECT agreement_id, state FROM smart_contracts WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let agreement_id: String = sc.try_get("agreement_id")?;
    let sc_state: String = sc.try_get("state")?;
    if sc_state != "locked" {
        return Err(ApiError::Conflict("escrow not in locked state".into()));
    }
    let is_part = sqlx::query("SELECT 1 FROM agreement_participants WHERE agreement_id = ? AND user_id = ?")
        .bind(&agreement_id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await?;
    if is_part.is_none() {
        return Err(ApiError::Forbidden("not a participant".into()));
    }

    // both parties must co-sign the release (CIP-30). Track via a release tx record.
    let now = now_iso();
    let agreement_value: i64 = sqlx::query("SELECT agreement_value FROM agreements WHERE id = ?")
        .bind(&agreement_id)
        .fetch_one(&state.pool)
        .await?
        .try_get("agreement_value")?;

    let txid = random_uuid();
    sqlx::query("INSERT INTO contract_transactions (id, smart_contract_id, kind, tx_cbor, status, created_at) VALUES (?, ?, 'release', NULL, 'awaiting_signatures', ?)")
        .bind(&txid)
        .bind(&id)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    sqlx::query("UPDATE smart_contracts SET state='releasing', updated_at=? WHERE id=?")
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;
    sqlx::query("UPDATE agreements SET status='releasing', updated_at=? WHERE id=?")
        .bind(&now)
        .bind(&agreement_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({
        "smart_contract_id": id,
        "agreement_id": agreement_id,
        "agreement_value": agreement_value,
        "release_tx_id": txid,
        "requires_cosign": ["buyer", "supplier"],
        "next": "POST /escrow/{id}/release with CIP-30 partial signatures from both wallets",
    })))
}

#[derive(Deserialize)]
pub struct ReleaseReq {
    /// map of user_id -> hex CIP-30 witness (partial transaction_witness_set)
    pub witnesses: std::collections::HashMap<String, String>,
}

/// `POST /escrow/:id/release` — submit both wallets' CIP-30 partial signatures, finalize release.
pub async fn release(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ReleaseReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let sc = sqlx::query("SELECT agreement_id, state FROM smart_contracts WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let agreement_id: String = sc.try_get("agreement_id")?;
    let sc_state: String = sc.try_get("state")?;
    if sc_state != "releasing" {
        return Err(ApiError::Conflict("call /escrow/{id}/complete first".into()));
    }

    let participants = sqlx::query("SELECT user_id, role FROM agreement_participants WHERE agreement_id = ?")
        .bind(&agreement_id)
        .fetch_all(&state.pool)
        .await?;
    let signed_ids: Vec<String> = req.witnesses.keys().cloned().collect();
    let mut all_signed = true;
    for p in &participants {
        let uid: String = p.try_get("user_id")?;
        if !signed_ids.contains(&uid) {
            all_signed = false;
        }
    }
    if !all_signed {
        return Err(ApiError::Conflict("both wallets must provide CIP-30 partial signatures".into()));
    }

    // build + submit release via the stub builder
    let builder = StubTxBuilder;
    let agreement_value: i64 = sqlx::query("SELECT agreement_value FROM agreements WHERE id = ?")
        .bind(&agreement_id)
        .fetch_one(&state.pool)
        .await?
        .try_get("agreement_value")?;
    let ctx = TxCtx {
        smart_contract_id: id.clone(),
        agreement_id: agreement_id.clone(),
        validator_addr: state.cfg.escrow_validator_addr.clone(),
        validator_hash: state.cfg.escrow_validator_script_hash.clone(),
        locked_amount: agreement_value,
        collateral_amount: 0,
        recipient: user.address.clone(),
        kind: "release".into(),
    };
    let draft = builder.build_release_tx(&ctx)?;
    let witness = json!(req.witnesses);
    let tx_hash = builder.submit(&draft.tx_cbor, &witness)?;

    let now = now_iso();
    sqlx::query("INSERT INTO contract_transactions (id, smart_contract_id, kind, tx_cbor, tx_hash, status, witness_party_ids, submitted_at, confirmed_at, created_at) VALUES (?, ?, 'release', ?, ?, 'confirmed', ?, ?, ?, ?)")
        .bind(random_uuid())
        .bind(&id)
        .bind(&draft.tx_cbor)
        .bind(&tx_hash)
        .bind(signed_ids.join(","))
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    sqlx::query("UPDATE smart_contracts SET state='completed', updated_at=? WHERE id=?")
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;
    sqlx::query("UPDATE agreements SET status='completed', updated_at=? WHERE id=?")
        .bind(&now)
        .bind(&agreement_id)
        .execute(&state.pool)
        .await?;

    // return collateral to both (no fault)
    collateral::return_collateral(&state, &agreement_id).await?;

    // award points to both participants (governance trust)
    points::award_to_participants(&state, &agreement_id, state.cfg.points_per_success, "contract_completed").await?;

    // anchor release + save receipt
    crate::modules::ledger::push(&state, "release", &agreement_id, &id, json!({
        "smart_contract_id": id,
        "agreement_id": agreement_id,
        "tx_hash": tx_hash,
        "amount": agreement_value,
    }), Some(&user.id)).await?;
    receipts::save_receipt(&state, &id, &user.id, json!({
        "agreement_id": agreement_id,
        "tx_hash": tx_hash,
        "amount": agreement_value,
        "kind": "release",
    })).await?;

    Ok(Json(json!({ "released": true, "tx_hash": tx_hash, "smart_contract_id": id })))
}

async fn fetch(state: &AppState, id: &str) -> ApiResult<Json<SmartContractResp>> {
    let r = sqlx::query("SELECT id, agreement_id, validator_hash, validator_addr, datum_hash, state, funded_so_far, total_required, funding_deadline FROM smart_contracts WHERE id = ?")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(SmartContractResp {
        id: r.try_get("id")?,
        agreement_id: r.try_get("agreement_id")?,
        validator_hash: r.try_get("validator_hash")?,
        validator_addr: r.try_get("validator_addr")?,
        datum_hash: r.try_get("datum_hash")?,
        state: r.try_get("state")?,
        funded_so_far: r.try_get("funded_so_far")?,
        total_required: r.try_get("total_required")?,
        funding_deadline: r.try_get("funding_deadline")?,
    }))
}

/// `GET /escrow/by-agreement/:agreement_id` — find the smart contract for an agreement.
pub async fn get_by_agreement(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(agreement_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let r = sqlx::query("SELECT id, state, validator_addr, deal_datum_json, total_required, funded_so_far FROM smart_contracts WHERE agreement_id = ? ORDER BY created_at DESC LIMIT 1")
        .bind(&agreement_id)
        .fetch_optional(&state.pool)
        .await?;

    match r {
        Some(row) => {
            let id: String = row.try_get("id")?;
            let state_str: String = row.try_get("state")?;
            let validator_addr: String = row.try_get("validator_addr")?;
            let deal_datum_json: String = row.try_get("deal_datum_json")?;
            let total_required: i64 = row.try_get("total_required")?;
            let funded_so_far: i64 = row.try_get("funded_so_far")?;
            Ok(Json(json!({
                "found": true,
                "id": id,
                "state": state_str,
                "validator_addr": validator_addr,
                "deal_datum": serde_json::from_str::<Value>(&deal_datum_json).unwrap_or(json!({})),
                "total_required": total_required,
                "funded_so_far": funded_so_far,
            })))
        }
        None => Ok(Json(json!({ "found": false }))),
    }
}

/// `POST /escrow/:id/build-spend-tx` — builds a spend tx (ClaimUnit) via Pallas.
/// Body: { action: "ClaimUnit", unit_id: "...", recipient: "addr..." }
#[derive(Deserialize)]
pub struct BuildSpendReq {
    pub action: String,
    pub unit_id: Option<String>,
    pub recipient: Option<String>,
}

pub async fn build_spend_tx(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<BuildSpendReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let sc = sqlx::query("SELECT validator_addr, deal_datum_json, total_required, funded_so_far, state FROM smart_contracts WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let validator_addr: String = sc.try_get("validator_addr")?;
    let deal_datum_str: String = sc.try_get("deal_datum_json")?;
    let deal_datum: Value = serde_json::from_str(&deal_datum_str).unwrap_or(json!({}));
    let sc_state: String = sc.try_get("state")?;

    // Find the release unit being claimed
    let release_units = deal_datum.get("release_units").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let unit = release_units.iter().find(|u| {
        u.get("unit_id").and_then(|v| v.as_str()) == req.unit_id.as_deref()
    }).or_else(|| release_units.iter().find(|u| {
        u.get("claimed").and_then(|v| v.as_bool()) == Some(false)
    }));

    let payout_amount = unit.and_then(|u| u.get("allocation")).and_then(|a| a.get("amount")).and_then(|v| v.as_i64()).unwrap_or(0);
    let recipient = req.recipient.unwrap_or_else(|| user.address.clone());

    // Build updated datum with the unit marked as claimed
    let updated_datum = {
        let mut units = release_units.clone();
        if let Some(u) = units.iter_mut().find(|u| {
            u.get("unit_id").and_then(|v| v.as_str()) == req.unit_id.as_deref()
        }) {
            u.as_object_mut().map(|m| m.insert("claimed".to_string(), json!(true)));
        }
        let mut d = deal_datum.clone();
        d.as_object_mut().map(|m| m.insert("release_units".to_string(), json!(units)));
        d
    };

    let remaining_amount: i64 = updated_datum.get("release_units")
        .and_then(|v| v.as_array())
        .map(|units| units.iter()
            .filter(|u| u.get("claimed").and_then(|v| v.as_bool()) != Some(true))
            .map(|u| u.get("allocation").and_then(|a| a.get("amount")).and_then(|v| v.as_i64()).unwrap_or(0))
            .sum())
        .unwrap_or(0);

    let datum_hex = hex::encode(serde_json::to_vec(&updated_datum).unwrap_or_default());

    // Build the spend tx via Pallas
    let tx_cbor = crate::modules::tx_builder::build_spend_tx(
        &crate::modules::koios::KoiosProvider::new(),
        &validator_addr,
        Some(&recipient),
        Some(payout_amount as u64),
        if remaining_amount > 0 { Some(&datum_hex) } else { None },
        if remaining_amount > 0 { Some(remaining_amount as u64) } else { None },
        &user.address,
        // Redeemer + language_view_cbor — TODO: build proper redeemer for ClaimUnit
        // For now, pass a placeholder so the function compiles. The spend tx won't
        // work until this is properly wired, but the LOCK tx (deposit) works now.
        &pallas::ledger::primitives::babbage::Redeemer {
            tag: pallas::ledger::primitives::babbage::RedeemerTag::Spend,
            index: 0,
            data: pallas::ledger::primitives::alonzo::PlutusData::Constr(
                pallas::ledger::primitives::alonzo::Constr { tag: 1, any_constructor: None, fields: vec![] }
            ),
            ex_units: pallas::ledger::primitives::babbage::ExUnits { mem: 1000000, steps: 100000000 },
        },
        &[], // empty language_view_cbor — will error if called, but lock tx doesn't use this
    ).await.map_err(|e| ApiError::Internal(anyhow::anyhow!("Pallas build_spend_tx failed: {}", e)))?;

    Ok(Json(json!({
        "tx_cbor": tx_cbor,
        "payout_amount": payout_amount,
        "recipient": recipient,
        "remaining_amount": remaining_amount,
    })))
}

/// `POST /escrow/:id/submit-spend-tx` — assembles + submits a signed spend tx.
#[derive(Deserialize)]
pub struct SubmitSpendReq {
    pub tx_cbor: String,       // the unsigned tx from build-spend-tx
    pub witness: String,       // the witness from wallet.signTx
}

pub async fn submit_spend_tx(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSpendReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let now = now_iso();

    // Assemble the signed tx
    let signed_cbor = crate::modules::tx_builder::assemble_signed_tx(&req.tx_cbor, &req.witness)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("Assemble signed spend tx failed: {}", e)))?;

    // Submit to Cardano Preprod via Koios
    let koios = crate::modules::koios::KoiosProvider::new();
    let tx_hash = koios.submit_tx(&signed_cbor).await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("Koios submit spend tx failed: {}", e)))?;

    // Update the agreement status
    let agreement_id: String = sqlx::query("SELECT agreement_id FROM smart_contracts WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?
        .try_get("agreement_id")?;

    sqlx::query("UPDATE smart_contracts SET state='completed', updated_at=? WHERE id=?")
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    sqlx::query("UPDATE agreements SET status='completed', updated_at=? WHERE id=?")
        .bind(&now)
        .bind(&agreement_id)
        .execute(&state.pool)
        .await?;

    // Award points
    crate::modules::points::award_to_participants(&state, &agreement_id, state.cfg.points_per_success, "contract_completed").await.ok();

    // Anchor on ledger
    crate::modules::ledger::push(&state, "release", &agreement_id, &id, json!({
        "smart_contract_id": id,
        "agreement_id": agreement_id,
        "tx_hash": tx_hash,
    }), Some(&user.id)).await.ok();

    Ok(Json(json!({
        "released": true,
        "tx_hash": tx_hash,
        "smart_contract_id": id,
    })))
}
