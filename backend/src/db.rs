use crate::error::ApiError;
use serde_json::json;
use sha2::{Digest, Sha256};

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn expires_iso(seconds: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339()
}

/// Canonical JSON serialization for hashing/signing: deterministic key order, no whitespace.
pub fn canonical_json(value: &serde_json::Value) -> String {
    canonical_value(value)
}

fn canonical_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => {
            let mut items: Vec<(String, String)> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonical_value(v)))
                .collect();
            items.sort_by(|a, b| a.0.cmp(&b.0));
            let body = items
                .iter()
                .map(|(k, v)| format!("{}:{}", json!(k), v))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{}}}", body)
        }
        serde_json::Value::Array(arr) => {
            let body = arr.iter().map(canonical_value).collect::<Vec<_>>().join(",");
            format!("[{}]", body)
        }
        other => other.to_string(),
    }
}

/// Blake2b-256 hex digest (Cardano's native hash).
pub fn blake2b_256_hex(data: &[u8]) -> String {
    type Blake2b256 = blake2::Blake2b<blake2::digest::consts::U32>;
    use blake2::Digest;
    let mut h = Blake2b256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// SHA-256 hex digest (used as a secondary content hash for the immutable ledger mirror).
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// Hash a structured payload the same way the wallet signs it: canonical JSON -> bytes.
pub fn payload_hash_hex(value: &serde_json::Value) -> Result<String, ApiError> {
    let canon = canonical_json(value);
    Ok(blake2b_256_hex(canon.as_bytes()))
}

pub fn random_hex(n_bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; n_bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn random_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn short_code(len: usize) -> String {
    use rand::seq::SliceRandom;
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| *ALPHABET.choose(&mut rng).unwrap() as char)
        .collect()
}

pub fn json_ok<T: serde::Serialize>(t: T) -> axum::Json<serde_json::Value> {
    axum::Json(json!({ "ok": true, "data": t }))
}

#[allow(dead_code)]
pub fn _unused() {
    let _ = json!(0);
}
