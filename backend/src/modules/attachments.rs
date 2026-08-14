//! Attachments + proof requirements + bounded resubmission per spec.
//!
//! Two kinds of attachments:
//! 1. Supporting material — Party 1 attaches a spec/brief as an exhibit
//! 2. Proof required — a milestone requires the counterparty to submit proof before release
//!
//! Bounded resubmission: 1st + 2nd rejection → resubmit, no penalty. 3rd rejection → disputed.
//! Rejection reason is mandatory. Full history visible to Party 2 + arbiter.

use crate::db::{now_iso, random_uuid};
use crate::error::{ApiError, ApiResult};
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;

// ---- Attachments (supporting material + proof submissions) ----

#[derive(Deserialize)]
pub struct UploadAttachmentReq {
    pub agreement_id: String,
    pub milestone_index: Option<i64>,   // None = agreement-level
    pub filename: String,
    pub file_type: String,               // document | image | link
    pub file_size: Option<i64>,
    pub content_hash: String,            // SHA-256 hex, computed client-side
    pub label: Option<String>,
    pub purpose: String,                  // supporting | proof
    pub url: Option<String>,             // link to the file (Drive/Dropbox/IPFS) — not stored on-chain
}

#[derive(Serialize)]
pub struct AttachmentResp {
    pub id: String,
    pub agreement_id: String,
    pub milestone_index: Option<i64>,
    pub filename: String,
    pub file_type: String,
    pub file_size: Option<i64>,
    pub content_hash: String,
    pub uploaded_by: String,
    pub label: Option<String>,
    pub purpose: String,
    pub created_at: String,
}

/// `POST /attachments` — upload an attachment (hash only, file stays off-chain).
pub async fn upload(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<UploadAttachmentReq>,
) -> ApiResult<Json<AttachmentResp>> {
    let id = random_uuid();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO attachments (id, agreement_id, milestone_index, filename, file_type, file_size, content_hash, uploaded_by, label, purpose, storage_url, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.agreement_id)
    .bind(req.milestone_index)
    .bind(&req.filename)
    .bind(&req.file_type)
    .bind(req.file_size)
    .bind(&req.content_hash)
    .bind(&user.id)
    .bind(&req.label)
    .bind(&req.purpose)
    .bind(&req.url)           // store the link (Drive/Dropbox/IPFS)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    fetch_attachment(&state, &id).await
}

/// `GET /attachments?agreement_id=...` — list attachments for an agreement.
pub async fn list(
    _user: AuthUser,
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT * FROM attachments WHERE agreement_id = ? ORDER BY created_at ASC")
        .bind(&q.agreement_id)
        .fetch_all(&state.pool)
        .await?;
    let v: Vec<Value> = rows.iter().map(|r| json!({
        "id": r.try_get::<String, _>("id").unwrap_or_default(),
        "milestone_index": r.try_get::<Option<i64>, _>("milestone_index").ok().flatten(),
        "filename": r.try_get::<String, _>("filename").unwrap_or_default(),
        "file_type": r.try_get::<String, _>("file_type").unwrap_or_default(),
        "file_size": r.try_get::<Option<i64>, _>("file_size").ok().flatten(),
        "content_hash": r.try_get::<String, _>("content_hash").unwrap_or_default(),
        "label": r.try_get::<Option<String>, _>("label").ok().flatten(),
        "purpose": r.try_get::<String, _>("purpose").unwrap_or_default(),
        "url": r.try_get::<Option<String>, _>("storage_url").ok().flatten(),
        "uploaded_by": r.try_get::<String, _>("uploaded_by").unwrap_or_default(),
        "created_at": r.try_get::<String, _>("created_at").unwrap_or_default(),
    })).collect();
    Ok(Json(json!({ "attachments": v })))
}

#[derive(Deserialize)]
pub struct ListQuery { pub agreement_id: String }

// ---- Proof requirements (milestone-level gates) ----

#[derive(Deserialize)]
pub struct SetProofReq {
    pub agreement_id: String,
    pub milestone_index: i64,
    pub kind: String,                   // document | image | link
    pub label: Option<String>,
    pub max_attempts: Option<i64>,       // default 3
}

/// `POST /proofs/require` — Party 1 sets a proof requirement on a milestone.
pub async fn set_requirement(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<SetProofReq>,
) -> ApiResult<Json<Value>> {
    let id = random_uuid();
    let max_attempts = req.max_attempts.unwrap_or(3).max(1);
    let now = now_iso();
    sqlx::query(
        "INSERT INTO proof_requirements (id, agreement_id, milestone_index, required, kind, label, max_attempts, rejection_count, status)
         VALUES (?, ?, ?, 1, ?, ?, ?, 0, 'pending')
         ON CONFLICT(agreement_id, milestone_index) DO UPDATE SET
           required=1, kind=excluded.kind, label=excluded.label, max_attempts=excluded.max_attempts",
    )
    .bind(&id)
    .bind(&req.agreement_id)
    .bind(req.milestone_index)
    .bind(&req.kind)
    .bind(&req.label)
    .bind(max_attempts)
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "set": true, "agreement_id": req.agreement_id, "milestone_index": req.milestone_index, "max_attempts": max_attempts })))
}

/// `GET /proofs/requirements?agreement_id=...` — list proof requirements for an agreement.
pub async fn list_requirements(
    _user: AuthUser,
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<ProofListQuery>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT * FROM proof_requirements WHERE agreement_id = ?")
        .bind(&q.agreement_id)
        .fetch_all(&state.pool)
        .await?;
    let v: Vec<Value> = rows.iter().map(|r| json!({
        "id": r.try_get::<String, _>("id").unwrap_or_default(),
        "milestone_index": r.try_get::<i64, _>("milestone_index").unwrap_or(0),
        "required": r.try_get::<i64, _>("required").unwrap_or(0) == 1,
        "kind": r.try_get::<String, _>("kind").unwrap_or_default(),
        "label": r.try_get::<Option<String>, _>("label").ok().flatten(),
        "max_attempts": r.try_get::<i64, _>("max_attempts").unwrap_or(3),
        "rejection_count": r.try_get::<i64, _>("rejection_count").unwrap_or(0),
        "status": r.try_get::<String, _>("status").unwrap_or_default(),
    })).collect();
    Ok(Json(json!({ "requirements": v })))
}

#[derive(Deserialize)]
pub struct ProofListQuery { pub agreement_id: String }

// ---- Proof submissions (counterparty uploads proof, Party 1 reviews) ----

#[derive(Deserialize)]
pub struct SubmitProofReq {
    pub agreement_id: String,
    pub milestone_index: i64,
    pub attachment_id: String,            // must be an existing attachment with purpose='proof'
    pub attachment_hash: String,
}

/// `POST /proofs/submit` — counterparty submits proof for a milestone.
pub async fn submit_proof(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<SubmitProofReq>,
) -> ApiResult<Json<Value>> {
    let id = random_uuid();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO proof_submissions (id, agreement_id, milestone_index, attachment_id, attachment_hash, submitted_by, submitted_at, outcome)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending')",
    )
    .bind(&id)
    .bind(&req.agreement_id)
    .bind(req.milestone_index)
    .bind(&req.attachment_id)
    .bind(&req.attachment_hash)
    .bind(&user.id)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    // reset requirement status to pending
    sqlx::query("UPDATE proof_requirements SET status='pending' WHERE agreement_id = ? AND milestone_index = ?")
        .bind(&req.agreement_id)
        .bind(req.milestone_index)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "submitted": true, "submission_id": id })))
}

/// `GET /proofs/submissions?agreement_id=...` — list all submissions (full history for arbiter).
pub async fn list_submissions(
    _user: AuthUser,
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<ProofListQuery>,
) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT * FROM proof_submissions WHERE agreement_id = ? ORDER BY submitted_at ASC")
        .bind(&q.agreement_id)
        .fetch_all(&state.pool)
        .await?;
    let v: Vec<Value> = rows.iter().map(|r| json!({
        "id": r.try_get::<String, _>("id").unwrap_or_default(),
        "milestone_index": r.try_get::<i64, _>("milestone_index").unwrap_or(0),
        "attachment_id": r.try_get::<String, _>("attachment_id").unwrap_or_default(),
        "attachment_hash": r.try_get::<String, _>("attachment_hash").unwrap_or_default(),
        "submitted_by": r.try_get::<String, _>("submitted_by").unwrap_or_default(),
        "submitted_at": r.try_get::<String, _>("submitted_at").unwrap_or_default(),
        "reviewed_at": r.try_get::<Option<String>, _>("reviewed_at").ok().flatten(),
        "outcome": r.try_get::<String, _>("outcome").unwrap_or_default(),
        "rejection_reason": r.try_get::<Option<String>, _>("rejection_reason").ok().flatten(),
    })).collect();
    Ok(Json(json!({ "submissions": v })))
}

#[derive(Deserialize)]
pub struct ReviewProofReq {
    pub submission_id: String,
    pub outcome: String,                 // accepted | rejected
    pub rejection_reason: Option<String>,  // mandatory if rejected
}

/// `POST /proofs/review` — Party 1 accepts or rejects a submission. 3rd rejection → disputed.
pub async fn review_proof(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<ReviewProofReq>,
) -> ApiResult<Json<Value>> {
    if req.outcome == "rejected" && (req.rejection_reason.is_none() || req.rejection_reason.as_ref().unwrap().is_empty()) {
        return Err(ApiError::BadRequest("rejection reason is mandatory".into()));
    }

    let now = now_iso();
    let sub = sqlx::query("SELECT agreement_id, milestone_index FROM proof_submissions WHERE id = ?")
        .bind(&req.submission_id)
        .fetch_one(&state.pool)
        .await?;
    let agreement_id: String = sub.try_get("agreement_id")?;
    let milestone_index: i64 = sub.try_get("milestone_index")?;

    // update submission outcome
    sqlx::query("UPDATE proof_submissions SET outcome = ?, rejection_reason = ?, reviewed_at = ? WHERE id = ?")
        .bind(&req.outcome)
        .bind(&req.rejection_reason)
        .bind(&now)
        .bind(&req.submission_id)
        .execute(&state.pool)
        .await?;

    if req.outcome == "accepted" {
        // proof accepted — milestone can proceed
        sqlx::query("UPDATE proof_requirements SET status = 'accepted' WHERE agreement_id = ? AND milestone_index = ?")
            .bind(&agreement_id)
            .bind(milestone_index)
            .execute(&state.pool)
            .await?;
        return Ok(Json(json!({ "outcome": "accepted", "milestone_can_release": true })));
    }

    // rejected — increment rejection count
    sqlx::query("UPDATE proof_requirements SET rejection_count = rejection_count + 1 WHERE agreement_id = ? AND milestone_index = ?")
        .bind(&agreement_id)
        .bind(milestone_index)
        .execute(&state.pool)
        .await?;

    let req_row = sqlx::query("SELECT rejection_count, max_attempts FROM proof_requirements WHERE agreement_id = ? AND milestone_index = ?")
        .bind(&agreement_id)
        .bind(milestone_index)
        .fetch_one(&state.pool)
        .await?;
    let rejection_count: i64 = req_row.try_get("rejection_count")?;
    let max_attempts: i64 = req_row.try_get("max_attempts")?;

    if rejection_count >= max_attempts {
        // 3rd (or final) rejection → disputed
        sqlx::query("UPDATE proof_requirements SET status = 'disputed' WHERE agreement_id = ? AND milestone_index = ?")
            .bind(&agreement_id)
            .bind(milestone_index)
            .execute(&state.pool)
            .await?;
        // mark agreement as disputed
        sqlx::query("UPDATE agreements SET status = 'disputed', updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&agreement_id)
            .execute(&state.pool)
            .await?;
        Ok(Json(json!({
            "outcome": "rejected",
            "rejection_count": rejection_count,
            "max_attempts": max_attempts,
            "milestone_status": "disputed",
            "message": "Final resubmission rejected — milestone moved to dispute"
        })))
    } else {
        sqlx::query("UPDATE proof_requirements SET status = 'rejected' WHERE agreement_id = ? AND milestone_index = ?")
            .bind(&agreement_id)
            .bind(milestone_index)
            .execute(&state.pool)
            .await?;
        let remaining = max_attempts - rejection_count;
        Ok(Json(json!({
            "outcome": "rejected",
            "rejection_count": rejection_count,
            "max_attempts": max_attempts,
            "resubmissions_remaining": remaining,
            "milestone_status": "rejected",
            "message": format!("Rejected — {} of {} resubmissions remaining", remaining, max_attempts)
        })))
    }
}

// ---- Helpers ----

async fn fetch_attachment(state: &AppState, id: &str) -> ApiResult<Json<AttachmentResp>> {
    let r = sqlx::query("SELECT * FROM attachments WHERE id = ?")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(AttachmentResp {
        id: r.try_get("id")?,
        agreement_id: r.try_get("agreement_id")?,
        milestone_index: r.try_get("milestone_index")?,
        filename: r.try_get("filename")?,
        file_type: r.try_get("file_type")?,
        file_size: r.try_get("file_size")?,
        content_hash: r.try_get("content_hash")?,
        uploaded_by: r.try_get("uploaded_by")?,
        label: r.try_get("label")?,
        purpose: r.try_get("purpose")?,
        created_at: r.try_get("created_at")?,
    }))
}

// ---- Milestone delivery status tracking ----

/// `GET /milestones?agreement_id=...` — list milestone statuses for an agreement.
pub async fn list_milestone_statuses(
    _user: AuthUser,
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<MilestoneQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let ag = sqlx::query("SELECT terms_json FROM agreements WHERE id = ?")
        .bind(&q.agreement_id)
        .fetch_one(&state.pool)
        .await?;
    let terms_json: String = ag.try_get("terms_json")?;
    let terms: serde_json::Value = serde_json::from_str(&terms_json).unwrap_or(json!({}));
    let milestones = terms.get("milestones").and_then(|m| m.as_array()).cloned().unwrap_or_default();

    // fetch proof requirements + submissions for each milestone
    let reqs = sqlx::query("SELECT milestone_index, status, rejection_count, max_attempts FROM proof_requirements WHERE agreement_id = ?")
        .bind(&q.agreement_id)
        .fetch_all(&state.pool)
        .await?;
    let subs = sqlx::query("SELECT milestone_index, outcome, rejection_reason, submitted_at FROM proof_submissions WHERE agreement_id = ? ORDER BY submitted_at ASC")
        .bind(&q.agreement_id)
        .fetch_all(&state.pool)
        .await?;

    let result: Vec<serde_json::Value> = milestones.iter().enumerate().map(|(i, m)| {
        let req_row = reqs.iter().find(|r| r.try_get::<i64, _>("milestone_index").unwrap_or(-1) == i as i64);
        let proof_status = req_row.and_then(|r| r.try_get::<String, _>("status").ok());
        let rejection_count = req_row.and_then(|r| r.try_get::<i64, _>("rejection_count").ok()).unwrap_or(0);
        let max_attempts = req_row.and_then(|r| r.try_get::<i64, _>("max_attempts").ok()).unwrap_or(3);

        let milestone_subs: Vec<serde_json::Value> = subs.iter()
            .filter(|s| s.try_get::<i64, _>("milestone_index").unwrap_or(-1) == i as i64)
            .map(|s| json!({
                "outcome": s.try_get::<String, _>("outcome").unwrap_or_default(),
                "rejection_reason": s.try_get::<String, _>("rejection_reason").unwrap_or_default(),
                "submitted_at": s.try_get::<String, _>("submitted_at").unwrap_or_default(),
            }))
            .collect();

        json!({
            "index": i,
            "label": m.get("label").and_then(|v| v.as_str()).unwrap_or(""),
            "percent": m.get("percent").and_then(|v| v.as_i64()).unwrap_or(0),
            "due": m.get("due").and_then(|v| v.as_str()).unwrap_or(""),
            "deliverables": m.get("deliverables").and_then(|v| v.as_str()).unwrap_or(""),
            "proof_required": m.get("proof").and_then(|p| p.get("required")).and_then(|v| v.as_bool()).unwrap_or(false),
            "proof_kind": m.get("proof").and_then(|p| p.get("kind")).and_then(|v| v.as_str()).unwrap_or(""),
            "proof_label": m.get("proof").and_then(|p| p.get("label")).and_then(|v| v.as_str()).unwrap_or(""),
            "proof_status": proof_status,
            "rejection_count": rejection_count,
            "max_attempts": max_attempts,
            "submissions": milestone_subs,
            "delivery_status": if proof_status.as_deref() == Some("accepted") { "accepted" }
                else if proof_status.as_deref() == Some("disputed") { "disputed" }
                else if proof_status.as_deref() == Some("rejected") { "rejected" }
                else if !milestone_subs.is_empty() { "pending_review" }
                else { "pending_delivery" },
        })
    }).collect();

    Ok(Json(json!({ "milestones": result })))
}

#[derive(Deserialize)]
pub struct MilestoneQuery { pub agreement_id: String }
