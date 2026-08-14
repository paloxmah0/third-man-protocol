use crate::error::ApiError;
use crate::modules::*;
use crate::AppState;
use axum::routing::{get, post};
use axum::Router;

pub fn routes() -> Router<AppState> {
    let auth = Router::new()
        .route("/auth/challenge", post(auth::challenge))
        .route("/auth/verify", post(auth::verify))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me));

    let profile = Router::new()
        .route("/profile", post(kyc::upsert_profile).get(kyc::my_profile))
        .route("/profile/privacy", axum::routing::patch(kyc::update_privacy))
        .route("/profile/:user_id", get(kyc::view_profile))
        .route("/kyc", get(kyc::my_kyc))
        .route("/kyc/submit", post(kyc::submit_kyc))
        .route("/kyc/verify", post(kyc::verify_kyc));

    let agreements = Router::new()
        .route("/agreements", post(agreements::create).get(agreements::list))
        .route("/agreements/:id", get(agreements::get).delete(agreements::delete))
        .route("/agreements/:id/terms", axum::routing::patch(agreements::update_terms))
        .route("/agreements/:id/revisions", get(agreements::list_revisions))
        .route("/agreements/:id/participants", get(otp::participants))
        .route("/agreements/:id/signable", get(signing::signable))
        .route("/agreements/:id/sign", post(signing::sign))
        .route("/agreements/:id/signatures", get(signing::list_signatures))
        .route("/agreements/:id/accept-terms", post(negotiation::accept_terms))
        .route("/agreements/:id/negotiation", get(negotiation::status))
        .route("/agreements/:id/collateral", get(collateral::list_for_agreement));

    let otp = Router::new()
        .route("/otp", post(otp::create))
        .route("/otp/redeem", post(otp::redeem));

    let attachments = Router::new()
        .route("/attachments", post(attachments::upload).get(attachments::list))
        .route("/proofs/require", post(attachments::set_requirement))
        .route("/proofs/requirements", get(attachments::list_requirements))
        .route("/proofs/submit", post(attachments::submit_proof))
        .route("/proofs/submissions", get(attachments::list_submissions))
        .route("/proofs/review", post(attachments::review_proof))
        .route("/milestones", get(attachments::list_milestone_statuses));

    let collateral_routes = Router::new()
        .route("/collateral/lock", post(collateral::lock))
        .route("/collateral/submit", post(collateral::submit_collateral));

    let escrow = Router::new()
        .route("/escrow/init", post(escrow::init))
        .route("/escrow/:id/lock-tx", get(escrow::build_lock_tx))
        .route("/escrow/:id/submit-lock-tx", post(escrow::submit_lock_tx))
        .route("/escrow/by-agreement/:agreement_id", get(escrow::get_by_agreement))
        .route("/escrow/:id/build-spend-tx", post(escrow::build_spend_tx))
        .route("/escrow/:id/submit-spend-tx", post(escrow::submit_spend_tx))
        .route("/escrow/:id/complete", post(escrow::complete))
        .route("/escrow/:id/release", post(escrow::release));

    let dispute = Router::new()
        .route("/disputes", post(dispute::raise))
        .route("/disputes/:id", get(dispute::get))
        .route("/disputes/:id/oracle", post(dispute::pull_oracle))
        .route("/disputes/:id/verdict", post(dispute::submit_verdict))
        .route("/arbiters/enroll", post(dispute::enroll_arbiter))
        .route("/arbiters", get(dispute::list_arbiters));

    let points = Router::new()
        .route("/points", get(points::my_balance))
        .route("/points/ledger", get(points::my_ledger));

    let receipts = Router::new()
        .route("/receipts", get(receipts::list_mine))
        .route("/receipts/:id", get(receipts::get));

    let ledger = Router::new()
        .route("/ledger/push", post(ledger::push_handler))
        .route("/ledger", get(ledger::list))
        .route("/ledger/:tx_hash", get(ledger::get))
        .route("/ledger/:tx_hash/confirm", post(ledger::confirm));

    let health = Router::new().route("/health", get(health));

    Router::new()
        .merge(health)
        .merge(auth)
        .merge(profile)
        .merge(agreements)
        .merge(otp)
        .merge(attachments)
        .merge(collateral_routes)
        .merge(escrow)
        .merge(dispute)
        .merge(points)
        .merge(receipts)
        .merge(ledger)
}

async fn health() -> Result<axum::Json<serde_json::Value>, ApiError> {
    Ok(axum::Json(serde_json::json!({ "ok": true, "service": "third-man-backend" })))
}
