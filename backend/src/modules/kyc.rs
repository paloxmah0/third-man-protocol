//! Registration flow per spec:
//! Step 1 — Wallet connect (handled by auth module → DID minted)
//! Step 2 — Basic profile (display name, avatar, location, bio, multi-select roles,
//!          languages, professional links, deal defaults, org mode, verification signals)
//! Step 3 — Tiered KYC (Tier 0 default, Tier 1 phone+OTP, Tier 2 ID+selfie — optional,
//!          non-blocking; attestation hash committed on-chain)
//! Step 4 — Privacy/visibility preferences (per-field Public / Participants-only / Private)

use crate::db::{now_iso, random_uuid};
use crate::error::{ApiError, ApiResult};
use crate::state::AuthUser;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;

// ---- Step 2: Basic Profile ----

#[derive(Deserialize)]
pub struct UpsertProfileReq {
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub location: Option<String>,
    pub bio: Option<String>,
    pub role_types: Option<Vec<String>>,       // ["Developer","Buyer",...]
    pub languages: Option<Vec<String>>,
    pub professional_links: Option<Vec<Value>>,// [{type,url,visible}]
    pub settlement_rails: Option<Vec<String>>,  // ["ADA","M-Pesa","mixed"]
    pub deal_size_range: Option<String>,        // "<$100" | "$100-1k" | ...
    pub availability: Option<String>,
    pub org_name: Option<String>,
    pub org_type: Option<String>,
    pub org_members: Option<Vec<String>>,
    pub verified_signals: Option<Vec<Value>>,   // [{type,verified}]
}

#[derive(Serialize)]
pub struct ProfileResp {
    pub id: String,
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub location: Option<String>,
    pub bio: Option<String>,
    pub role_types: Vec<String>,
    pub languages: Vec<String>,
    pub professional_links: Vec<Value>,
    pub settlement_rails: Vec<String>,
    pub deal_size_range: Option<String>,
    pub availability: Option<String>,
    pub org_name: Option<String>,
    pub org_type: Option<String>,
    pub org_members: Vec<String>,
    pub verified_signals: Vec<Value>,
    pub privacy_prefs: Value,
    pub created_at: String,
    pub updated_at: String,
}

/// `POST /profile` — create or update basic profile (Step 2).
pub async fn upsert_profile(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<UpsertProfileReq>,
) -> ApiResult<Json<ProfileResp>> {
    let now = now_iso();
    let id = random_uuid();

    // clone before the struct is consumed by .bind() calls below
    let roles_for_user_update = req.role_types.clone().unwrap_or_default();

    let role_types = serde_json::to_string(&req.role_types.unwrap_or_default()).unwrap_or_default();
    let languages = serde_json::to_string(&req.languages.unwrap_or_default()).unwrap_or_default();
    let links = serde_json::to_string(&req.professional_links.unwrap_or_default()).unwrap_or_default();
    let rails = serde_json::to_string(&req.settlement_rails.unwrap_or_default()).unwrap_or_default();
    let org_members = serde_json::to_string(&req.org_members.unwrap_or_default()).unwrap_or_default();
    let signals = serde_json::to_string(&req.verified_signals.unwrap_or_default()).unwrap_or_default();
    let privacy_defaults = default_privacy_prefs();

    // upsert
    sqlx::query(
        "INSERT INTO profiles (id, user_id, display_name, avatar_url, location, bio, role_types, languages,
         professional_links, settlement_rails, deal_size_range, availability, org_name, org_type, org_members,
         verified_signals, privacy_prefs, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
           display_name=excluded.display_name, avatar_url=excluded.avatar_url, location=excluded.location,
           bio=excluded.bio, role_types=excluded.role_types, languages=excluded.languages,
           professional_links=excluded.professional_links, settlement_rails=excluded.settlement_rails,
           deal_size_range=excluded.deal_size_range, availability=excluded.availability,
           org_name=excluded.org_name, org_type=excluded.org_type, org_members=excluded.org_members,
           verified_signals=excluded.verified_signals, updated_at=excluded.updated_at",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&req.display_name)
    .bind(&req.avatar_url)
    .bind(&req.location)
    .bind(&req.bio)
    .bind(&role_types)
    .bind(&languages)
    .bind(&links)
    .bind(&rails)
    .bind(&req.deal_size_range)
    .bind(&req.availability)
    .bind(&req.org_name)
    .bind(&req.org_type)
    .bind(&org_members)
    .bind(&signals)
    .bind(&privacy_defaults)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    // update user role if role_types contains supplier or buyer (for agreement gating)
    let roles = roles_for_user_update;
    let primary_role = if roles.iter().any(|r| r == "supplier" || r == "Supplier") {
        "supplier"
    } else if roles.iter().any(|r| r == "buyer" || r == "Buyer") {
        "buyer"
    } else if roles.iter().any(|r| r == "Service Provider" || r == "Developer" || r == "Freelancer") {
        "supplier"
    } else {
        "buyer"
    };
    sqlx::query("UPDATE users SET role = ?, updated_at = ? WHERE id = ?")
        .bind(primary_role)
        .bind(&now)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;

    fetch_profile(&state, &user.id).await
}

/// `GET /profile` — current user's full profile.
pub async fn my_profile(user: AuthUser, State(state): State<AppState>) -> ApiResult<Json<ProfileResp>> {
    fetch_profile(&state, &user.id).await
}

/// `GET /profile/:user_id` — view another user's profile with privacy filtering applied.
pub async fn view_profile(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(target_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let r = sqlx::query("SELECT * FROM profiles WHERE user_id = ?")
        .bind(&target_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("profile not found".into()))?;

    let prefs: String = r.try_get("privacy_prefs")?;
    let prefs: Value = serde_json::from_str(&prefs).unwrap_or(json!({}));

    // For now, apply public-only filtering (participants check would need deal context)
    let filtered = json!({
        "display_name": filter_field(&r, "display_name", &prefs, "display_name"),
        "avatar_url": filter_field(&r, "avatar_url", &prefs, "avatar"),
        "location": filter_field(&r, "location", &prefs, "location"),
        "bio": filter_field(&r, "bio", &prefs, "bio"),
        "role_types": filter_field(&r, "role_types", &prefs, "role_types"),
        "languages": filter_field(&r, "languages", &prefs, "languages"),
        "professional_links": filter_field(&r, "professional_links", &prefs, "professional_links"),
        "deal_size_range": filter_field(&r, "deal_size_range", &prefs, "deal_size_range"),
        "settlement_rails": filter_field(&r, "settlement_rails", &prefs, "settlement_rails"),
        "org_name": filter_field(&r, "org_name", &prefs, "org_name"),
        "verified_signals": filter_field(&r, "verified_signals", &prefs, "verified_signals"),
    });
    Ok(Json(filtered))
}

// ---- Step 3: Tiered KYC ----

#[derive(Deserialize)]
pub struct SubmitKycReq {
    pub tier: i64,               // 1 or 2
    pub phone: Option<String>,    // Tier 1
    pub legal_name: Option<String>,// Tier 2
    pub document_type: Option<String>,
    pub document_hash: Option<String>,
    pub selfie_hash: Option<String>,
}

#[derive(Serialize)]
pub struct KycTierResp {
    pub id: String,
    pub user_id: String,
    pub tier: i64,
    pub status: String,
    pub phone: Option<String>,
    pub phone_verified: bool,
    pub legal_name: Option<String>,
    pub attestation_hash: Option<String>,
    pub issued_at: Option<String>,
    pub expiry_at: Option<String>,
}

/// `POST /kyc/submit` — submit for a KYC tier (1=phone, 2=ID+selfie).
pub async fn submit_kyc(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<SubmitKycReq>,
) -> ApiResult<Json<KycTierResp>> {
    if req.tier != 1 && req.tier != 2 {
        return Err(ApiError::BadRequest("tier must be 1 or 2".into()));
    }
    if req.tier == 1 && req.phone.is_none() {
        return Err(ApiError::BadRequest("phone required for tier 1".into()));
    }
    if req.tier == 2 && (req.legal_name.is_none() || req.document_hash.is_none()) {
        return Err(ApiError::BadRequest("legal_name + document_hash required for tier 2".into()));
    }

    let now = now_iso();
    let id = random_uuid();
    let status = if req.tier == 1 { "pending_t1" } else { "pending_t2" };

    sqlx::query(
        "INSERT INTO kyc_tiers (id, user_id, tier, phone, legal_name, document_type, document_hash, selfie_hash,
         status, submitted_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
           tier=excluded.tier, phone=excluded.phone, legal_name=excluded.legal_name,
           document_type=excluded.document_type, document_hash=excluded.document_hash,
           selfie_hash=excluded.selfie_hash, status=excluded.status, updated_at=excluded.updated_at",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&req.tier)
    .bind(&req.phone)
    .bind(&req.legal_name)
    .bind(&req.document_type)
    .bind(&req.document_hash)
    .bind(&req.selfie_hash)
    .bind(&status)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    fetch_kyc(&state, &user.id).await
}

/// `POST /kyc/verify` — operator/demo: verify a user's KYC tier.
#[derive(Deserialize)]
pub struct VerifyKycReq {
    pub user_id: String,
    pub status: String, // verified_t1 | verified_t2 | rejected
}
pub async fn verify_kyc(
    _user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<VerifyKycReq>,
) -> ApiResult<Json<KycTierResp>> {
    let now = now_iso();
    let (attestation, issued, expiry) = if req.status.starts_with("verified") {
        let hash = crate::db::blake2b_256_hex(format!("kyc:{}:{}", req.user_id, now).as_bytes());
        let exp = (chrono::Utc::now() + chrono::Duration::days(365)).to_rfc3339();
        (Some(hash), Some(now.clone()), Some(exp))
    } else {
        (None, None, None)
    };

    sqlx::query("UPDATE kyc_tiers SET status = ?, attestation_hash = ?, issued_at = ?, expiry_at = ?, updated_at = ? WHERE user_id = ?")
        .bind(&req.status)
        .bind(&attestation)
        .bind(&issued)
        .bind(&expiry)
        .bind(&now)
        .bind(&req.user_id)
        .execute(&state.pool)
        .await?;

    fetch_kyc(&state, &req.user_id).await
}

/// `GET /kyc` — current user's KYC tier status.
pub async fn my_kyc(user: AuthUser, State(state): State<AppState>) -> ApiResult<Json<KycTierResp>> {
    fetch_kyc(&state, &user.id).await
}

// ---- Step 4: Privacy Preferences ----

#[derive(Deserialize)]
pub struct UpdatePrivacyReq {
    pub prefs: Value, // {"display_name":"public","location":"participants_only",...}
}

/// `PATCH /profile/privacy` — update per-field visibility preferences.
pub async fn update_privacy(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<UpdatePrivacyReq>,
) -> ApiResult<Json<Value>> {
    let now = now_iso();
    let prefs_str = serde_json::to_string(&req.prefs).unwrap_or_default();
    sqlx::query("UPDATE profiles SET privacy_prefs = ?, updated_at = ? WHERE user_id = ?")
        .bind(&prefs_str)
        .bind(&now)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "updated": true, "prefs": req.prefs })))
}

// ---- Helpers ----

fn default_privacy_prefs() -> String {
    json!({
        "display_name": "public",
        "avatar": "public",
        "location": "public_country",
        "bio": "public",
        "role_types": "public",
        "languages": "public",
        "professional_links": "private",
        "deal_size_range": "participants_only",
        "settlement_rails": "participants_only",
        "org_members": "private",
        "verified_signals": "public",
        "kyc_tier": "public",
        "reputation": "public",
        "phone": "participants_only",
        "email": "participants_only",
        "deal_history": "private"
    }).to_string()
}

fn filter_field(r: &sqlx::sqlite::SqliteRow, col: &str, prefs: &Value, pref_key: &str) -> Value {
    let visibility = prefs.get(pref_key).and_then(|v| v.as_str()).unwrap_or("private");
    // For public viewers, only return fields marked "public" (or "public_country" for location)
    if visibility == "public" || visibility == "public_country" {
        let raw: String = r.try_get(col).unwrap_or_default();
        let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| Value::String(raw.clone()));
        if visibility == "public_country" && col == "location" {
            // return country only
            if let Some(s) = parsed.as_str() {
                if let Some(country) = s.split(',').nth(1) {
                    return Value::String(country.trim().to_string());
                }
            }
        }
        parsed
    } else {
        Value::Null
    }
}

async fn fetch_profile(state: &AppState, user_id: &str) -> ApiResult<Json<ProfileResp>> {
    let r = sqlx::query("SELECT * FROM profiles WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("profile not found".into()))?;

    Ok(Json(row_to_profile(&r)?))
}

async fn fetch_kyc(state: &AppState, user_id: &str) -> ApiResult<Json<KycTierResp>> {
    let r = sqlx::query("SELECT * FROM kyc_tiers WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("kyc not found".into()))?;

    Ok(Json(KycTierResp {
        id: r.try_get("id")?,
        user_id: r.try_get("user_id")?,
        tier: r.try_get("tier")?,
        status: r.try_get("status")?,
        phone: r.try_get("phone")?,
        phone_verified: r.try_get::<i64, _>("phone_verified")? != 0,
        legal_name: r.try_get("legal_name")?,
        attestation_hash: r.try_get("attestation_hash")?,
        issued_at: r.try_get("issued_at")?,
        expiry_at: r.try_get("expiry_at")?,
    }))
}

fn row_to_profile(r: &sqlx::sqlite::SqliteRow) -> ApiResult<ProfileResp> {
    let parse = |col: &str| -> Value {
        let s: String = r.try_get(col).unwrap_or_default();
        serde_json::from_str(&s).unwrap_or(Value::Null)
    };
    let privacy_prefs: String = r.try_get("privacy_prefs").unwrap_or_default();
    let privacy_prefs: Value = serde_json::from_str(&privacy_prefs).unwrap_or(json!({}));

    Ok(ProfileResp {
        id: r.try_get("id")?,
        user_id: r.try_get("user_id")?,
        display_name: r.try_get("display_name")?,
        avatar_url: r.try_get("avatar_url")?,
        location: r.try_get("location")?,
        bio: r.try_get("bio")?,
        role_types: parse("role_types").as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
        languages: parse("languages").as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
        professional_links: parse("professional_links").as_array().map(|a| a.to_vec()).unwrap_or_default(),
        settlement_rails: parse("settlement_rails").as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
        deal_size_range: r.try_get("deal_size_range")?,
        availability: r.try_get("availability")?,
        org_name: r.try_get("org_name")?,
        org_type: r.try_get("org_type")?,
        org_members: parse("org_members").as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
        verified_signals: parse("verified_signals").as_array().map(|a| a.to_vec()).unwrap_or_default(),
        privacy_prefs,
        created_at: r.try_get("created_at")?,
        updated_at: r.try_get("updated_at")?,
    })
}
