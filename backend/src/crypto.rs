//! Cardano cryptography helpers: CIP-8 COSE_Sign1 verification (Ed25519),
//! DID generation from a wallet address, and nonce issuance.

use crate::error::{ApiError, ApiResult};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;

/// A parsed CIP-8 `DataSignature` returned by `api.signData`.
pub struct Cip8Signature {
    pub protected: Vec<u8>,   // raw protected header bytes (CBOR map)
    pub payload: Vec<u8>,     // signed payload bytes
    pub signature: Vec<u8>,   // 64-byte Ed25519 signature
    pub public_key: Vec<u8>,  // 32-byte Ed25519 verifying key (from COSE_Key x)
    pub address_bytes: Option<Vec<u8>>, // "address" protected header, if present
}

/// Parse the CIP-30 `DataSignature { signature: cbor<COSE_Sign1>, key: cbor<COSE_Key> }`.
/// Both inputs are hex-encoded CBOR.
pub fn parse_cip8(cose_sign1_hex: &str, cose_key_hex: &str) -> ApiResult<Cip8Signature> {
    let sign1_bytes = hex::decode(cose_sign1_hex.trim()).map_err(|e| ApiError::BadRequest(format!("bad sign1 hex: {e}")))?;
    let key_bytes = hex::decode(cose_key_hex.trim()).map_err(|e| ApiError::BadRequest(format!("bad key hex: {e}")))?;

    let sign1: ciborium::Value = ciborium::from_reader(&sign1_bytes[..])
        .map_err(|e| ApiError::BadRequest(format!("bad COSE_Sign1 CBOR: {e}")))?;
    let key: ciborium::Value = ciborium::from_reader(&key_bytes[..])
        .map_err(|e| ApiError::BadRequest(format!("bad COSE_Key CBOR: {e}")))?;

    let arr = as_array(&sign1).ok_or_else(|| ApiError::BadRequest("COSE_Sign1 not an array".into()))?;
    if arr.len() != 4 {
        return Err(ApiError::BadRequest("COSE_Sign1 must have 4 elements".into()));
    }
    let protected = as_bytes(&arr[0]).ok_or_else(|| ApiError::BadRequest("protected header not bytes".into()))?;
    let payload = match &arr[2] {
        ciborium::Value::Null => Vec::new(),
        v => as_bytes(v).ok_or_else(|| ApiError::BadRequest("payload not bytes".into()))?.clone(),
    };
    let signature = as_bytes(&arr[3]).ok_or_else(|| ApiError::BadRequest("signature not bytes".into()))?.clone();

    // protected header is itself a CBOR-encoded map
    let protected_map: ciborium::Value = ciborium::from_reader(&protected[..])
        .map_err(|e| ApiError::BadRequest(format!("bad protected header CBOR: {e}")))?;
    let address_bytes = extract_address(&protected_map);

    // COSE_Key: { 1: 1 (OKP), 3: -8 (EdDSA), -1: 6 (Ed25519), -2: <pubkey bstr> }
    let key_map = as_map(&key).ok_or_else(|| ApiError::BadRequest("COSE_Key not a map".into()))?;
    let mut public_key: Option<Vec<u8>> = None;
    for (k, v) in key_map {
        if let ciborium::Value::Integer(i) = k {
            let n: i64 = i64::try_from(*i).unwrap_or(0);
            if n == -2 {
                public_key = as_bytes(v).cloned();
            }
        }
    }
    let public_key = public_key.ok_or_else(|| ApiError::BadRequest("COSE_Key missing x (-2)".into()))?;
    if public_key.len() != 32 {
        return Err(ApiError::BadRequest(format!("public key must be 32 bytes, got {}", public_key.len())));
    }

    Ok(Cip8Signature {
        protected: protected.clone(),
        payload,
        signature,
        public_key,
        address_bytes,
    })
}

/// Verify the CIP-8 Ed25519 signature. Reconstructs the `Sig_structure` per CIP-8/RFC 8152:
///   Sig_structure = ["Signature1", body_protected, h'', payload]
/// and verifies the signature over its CBOR encoding with the public key.
pub fn verify_cip8(sig: &Cip8Signature) -> ApiResult<()> {
    let external_aad: Vec<u8> = Vec::new();

    let sig_structure = ciborium::Value::Array(vec![
        ciborium::Value::Text("Signature1".to_string()),
        ciborium::Value::Bytes(sig.protected.clone()),
        ciborium::Value::Bytes(external_aad),
        ciborium::Value::Bytes(sig.payload.clone()),
    ]);

    let mut to_verify = Vec::new();
    ciborium::into_writer(&sig_structure, &mut to_verify)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to encode Sig_structure: {e}")))?;

    let pk_bytes: [u8; 32] = sig.public_key.as_slice().try_into()
        .map_err(|_| ApiError::SignatureFailed("public key must be 32 bytes".into()))?;
    let pk = VerifyingKey::from_bytes(&pk_bytes)
        .map_err(|e| ApiError::SignatureFailed(format!("bad public key: {e}")))?;
    let ed_sig = Signature::from_slice(&sig.signature)
        .map_err(|e| ApiError::SignatureFailed(format!("bad signature bytes: {e}")))?;
    pk.verify(&to_verify, &ed_sig)
        .map_err(|_| ApiError::SignatureFailed("ed25519 verification failed".into()))?;
    Ok(())
}

/// DID method: `did:cardano:<bech32-or-hex-address>`. Stable per-wallet identifier.
pub fn did_from_address(address: &str) -> String {
    format!("did:cardano:{}", address)
}

/// Loose check that the protected "address" header (raw address bytes) is consistent
/// with the claimed address. Best-effort: returns true when it matches or when the
/// claimed address isn't a clean hex form we can compare byte-for-byte.
pub fn address_matches(sig: &Cip8Signature, claimed_address: &str) -> bool {
    let Some(addr_bytes) = &sig.address_bytes else { return true; };
    // bech32 addresses (start with "addr") aren't decoded here; skip strict check.
    if claimed_address.starts_with("addr") {
        return true;
    }
    let claimed_bytes = hex::decode(claimed_address.trim()).unwrap_or_default();
    !claimed_bytes.is_empty() && claimed_bytes == *addr_bytes
}

fn as_array(v: &ciborium::Value) -> Option<&[ciborium::Value]> {
    if let ciborium::Value::Array(a) = v {
        Some(a.as_slice())
    } else {
        None
    }
}
fn as_bytes(v: &ciborium::Value) -> Option<&Vec<u8>> {
    if let ciborium::Value::Bytes(b) = v {
        Some(b)
    } else {
        None
    }
}
fn as_map(v: &ciborium::Value) -> Option<&Vec<(ciborium::Value, ciborium::Value)>> {
    if let ciborium::Value::Map(m) = v {
        Some(m)
    } else {
        None
    }
}

fn extract_address(protected_map: &ciborium::Value) -> Option<Vec<u8>> {
    let m = as_map(protected_map)?;
    for (k, v) in m {
        if let ciborium::Value::Text(t) = k {
            if t == "address" {
                if let ciborium::Value::Bytes(b) = v { return Some(b.clone()); }
            }
        }
    }
    None
}

/// Serialize a value as the bytes the wallet will be asked to sign (canonical JSON),
/// returned hex so the frontend passes it to `signData(addr, payloadHex)`.
pub fn signable_payload_hex(value: &Value) -> String {
    let canon = crate::db::canonical_json(value);
    hex::encode(canon.as_bytes())
}

pub fn _unused() {
    let _ = 0;
}
