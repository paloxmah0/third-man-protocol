//! Points / governance tokens. Awarded on successful contracts and verdicts; they
//! raise an arbiter's trust and weight them in the governance pool.

use crate::db::{now_iso, random_uuid};
use crate::error::ApiResult;
use crate::AppState;
use axum::extract::State;
use serde_json::Value;
use sqlx::Row;

/// Award points to a single user.
pub async fn award(
    state: &AppState,
    user_id: &str,
    delta: i64,
    reason: &str,
    ref_id: Option<&str>,
) -> ApiResult<()> {
    let now = now_iso();
    sqlx::query("INSERT INTO points_ledger (id, user_id, delta, reason, ref_id, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(random_uuid())
        .bind(user_id)
        .bind(delta)
        .bind(reason)
        .bind(ref_id)
        .bind(&now)
        .execute(&state.pool)
        .await?;

    sqlx::query("INSERT INTO points_balances (user_id, balance) VALUES (?, ?) ON CONFLICT(user_id) DO UPDATE SET balance = balance + ?")
        .bind(user_id)
        .bind(delta)
        .bind(delta)
        .execute(&state.pool)
        .await?;

    // arbiters also accrue trust points
    sqlx::query("UPDATE arbiters SET trust_points = trust_points + ? WHERE user_id = ?")
        .bind(delta)
        .bind(user_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// Award points to all participants of an agreement.
pub async fn award_to_participants(
    state: &AppState,
    agreement_id: &str,
    delta: i64,
    reason: &str,
) -> ApiResult<()> {
    let rows = sqlx::query("SELECT user_id FROM agreement_participants WHERE agreement_id = ?")
        .bind(agreement_id)
        .fetch_all(&state.pool)
        .await?;
    for r in rows {
        let uid: String = r.try_get("user_id")?;
        award(state, &uid, delta, reason, Some(agreement_id)).await?;
    }
    Ok(())
}

/// `GET /points` — current user's balance.
pub async fn my_balance(
    user: crate::state::AuthUser,
    State(state): State<AppState>,
) -> ApiResult<axum::Json<Value>> {
    let bal: i64 = sqlx::query("SELECT COALESCE((SELECT balance FROM points_balances WHERE user_id = ?), 0) as b")
        .bind(&user.id)
        .fetch_one(&state.pool)
        .await?
        .try_get("b")?;
    Ok(axum::Json(serde_json::json!({ "user_id": user.id, "points": bal })))
}

/// `GET /points/ledger` — current user's point history.
pub async fn my_ledger(
    user: crate::state::AuthUser,
    State(state): State<AppState>,
) -> ApiResult<axum::Json<Value>> {
    let rows = sqlx::query("SELECT delta, reason, ref_id, created_at FROM points_ledger WHERE user_id = ? ORDER BY created_at DESC")
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await?;
    let v: Vec<Value> = rows
        .iter()
        .map(|r| serde_json::json!({
            "delta": r.try_get::<i64, _>("delta").unwrap_or(0),
            "reason": r.try_get::<String, _>("reason").unwrap_or_default(),
            "ref_id": r.try_get::<Option<String>, _>("ref_id").ok().flatten(),
            "created_at": r.try_get::<String, _>("created_at").unwrap_or_default(),
        }))
        .collect();
    Ok(axum::Json(serde_json::json!({ "ledger": v })))
}
