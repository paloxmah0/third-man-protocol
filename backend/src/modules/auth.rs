//! Registration & login: a Cardano wallet proves ownership by signing a server-issued
//! nonce with CIP-8 `signData`. On success we mint a `did:cardano:<addr>` and a session.

use crate::crypto;
use crate::db::{expires_iso, now_iso, random_hex, random_uuid};
use crate::error::{ApiError, ApiResult};
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Deserialize)]
pub struct ChallengeReq {
    pub address: String,
    pub purpose: Option<String>, // register | login (default login)
}

#[derive(Serialize)]
pub struct ChallengeResp {
    pub challenge_id: String,
    pub nonce: String, // hex bytes the wallet must sign with signData
    pub purpose: String,
}

/// `POST /auth/challenge` — issue a nonce for a wallet address.
pub async fn challenge(
    State(state): State<AppState>,
    Json(req): Json<ChallengeReq>,
) -> ApiResult<Json<ChallengeResp>> {
    let purpose = req.purpose.unwrap_or_else(|| "login".to_string());
    let nonce = random_hex(16);
    let id = random_uuid();
    let now = now_iso();
    let exp = expires_iso(300); // 5 minutes

    sqlx::query(
        "INSERT INTO challenges (id, address, nonce, purpose, created_at, expires_at, used)
         VALUES (?, ?, ?, ?, ?, ?, 0)",
    )
    .bind(&id)
    .bind(&req.address)
    .bind(&nonce)
    .bind(&purpose)
    .bind(&now)
    .bind(&exp)
    .execute(&state.pool)
    .await?;

    Ok(Json(ChallengeResp {
        challenge_id: id,
        nonce,
        purpose,
    }))
}

#[derive(Deserialize)]
pub struct VerifyReq {
    pub challenge_id: String,
    pub cose_sign1: String, // hex CBOR COSE_Sign1
    pub cose_key: String,    // hex CBOR COSE_Key
}

#[derive(Serialize)]
pub struct SessionResp {
    pub token: String,
    pub user_id: String,
    pub did: String,
    pub address: String,
    pub role: String,
    pub new_user: bool,
}

/// `POST /auth/verify` — verify the CIP-8 signature over the nonce, create/fetch the
/// user, and issue a session token.
pub async fn verify(
    State(state): State<AppState>,
    Json(req): Json<VerifyReq>,
) -> ApiResult<Json<SessionResp>> {
    let sig = crypto::parse_cip8(&req.cose_sign1, &req.cose_key)?;
    crypto::verify_cip8(&sig)?;

    let row = sqlx::query("SELECT address, nonce, purpose, expires_at, used FROM challenges WHERE id = ?")
        .bind(&req.challenge_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::BadRequest("challenge not found".into()))?;

    let address: String = row.try_get("address")?;
    let nonce: String = row.try_get("nonce")?;
    let _purpose: String = row.try_get("purpose")?;
    let expires_at: String = row.try_get("expires_at")?;
    let used: i64 = row.try_get("used")?;

    if used != 0 {
        return Err(ApiError::BadRequest("challenge already used".into()));
    }
    if now_iso() > expires_at {
        return Err(ApiError::BadRequest("challenge expired".into()));
    }
    if hex::encode(&sig.payload) != nonce.to_lowercase() {
        return Err(ApiError::SignatureFailed("signed payload does not match nonce".into()));
    }
    if !crypto::address_matches(&sig, &address) {
        return Err(ApiError::SignatureFailed("address header mismatch".into()));
    }

    // mark challenge used
    sqlx::query("UPDATE challenges SET used = 1 WHERE id = ?")
        .bind(&req.challenge_id)
        .execute(&state.pool)
        .await?;

    let did = crypto::did_from_address(&address);
    let pubkey_hex = hex::encode(&sig.public_key);

    // upsert user
    let existing = sqlx::query("SELECT id, role FROM users WHERE address = ?")
        .bind(&address)
        .fetch_optional(&state.pool)
        .await?;

    let (user_id, role, new_user) = match existing {
        Some(r) => {
            let id: String = r.try_get("id")?;
            let role: String = r.try_get("role")?;
            sqlx::query("UPDATE users SET payment_pubkey = ?, updated_at = ? WHERE id = ?")
                .bind(&pubkey_hex)
                .bind(now_iso())
                .bind(&id)
                .execute(&state.pool)
                .await?;
            (id, role, false)
        }
        None => {
            let id = random_uuid();
            let now = now_iso();
            sqlx::query(
                "INSERT INTO users (id, did, address, payment_pubkey, role, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'unassigned', 'active', ?, ?)",
            )
            .bind(&id)
            .bind(&did)
            .bind(&address)
            .bind(&pubkey_hex)
            .bind(&now)
            .bind(&now)
            .execute(&state.pool)
            .await?;
            (id, "unassigned".to_string(), true)
        }
    };

    let token = random_hex(32);
    let now = now_iso();
    let exp = expires_iso(60 * 60 * 24 * 30); // 30 days
    sqlx::query("INSERT INTO sessions (token, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)")
        .bind(&token)
        .bind(&user_id)
        .bind(&now)
        .bind(&exp)
        .execute(&state.pool)
        .await?;

    Ok(Json(SessionResp {
        token,
        user_id,
        did,
        address,
        role,
        new_user,
    }))
}

/// `POST /auth/logout`
pub async fn logout(user: AuthUser, State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    // delete all sessions for this user (simple)
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    Ok(crate::db::json_ok(serde_json::json!({ "logged_out": true })))
}

/// `GET /auth/me`
pub async fn me(user: AuthUser) -> ApiResult<Json<serde_json::Value>> {
    Ok(crate::db::json_ok(serde_json::json!({
        "id": user.id,
        "did": user.did,
        "address": user.address,
        "role": user.role,
    })))
}
