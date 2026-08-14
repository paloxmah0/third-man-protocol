//! Koios API provider — fetches UTxOs, submits transactions, queries the chain.
//! All Cardano network interaction goes through here. Pure Rust, no browser.
//!
//! Fix #3: Koios' `inline_datum` field is NOT a flat hex string. It's an object:
//!   "inline_datum": { "bytes": "d8799f...", "value": { <decoded Plutus JSON> } }
//! or null if there's no inline datum. This file correctly unwraps it.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

const KOIOS_BASE_URL: &str = "https://preprod.koios.rest/api/v1";

pub struct KoiosProvider {
    client: reqwest::Client,
}

impl KoiosProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Fetches UTxOs for a (bech32!) address. Logs the raw response body on
    /// any parse failure so you can see the actual field names/shape Koios
    /// sent back, instead of guessing (fix #3).
    pub async fn get_utxos_at(&self, address: &str) -> Result<Vec<KoiosUtxo>> {
        let body = serde_json::json!({ "_addresses": [address] });

        let resp = self
            .client
            .post(format!("{KOIOS_BASE_URL}/address_info"))
            .json(&body)
            .send()
            .await
            .context("POST /address_info request failed")?;

        let status = resp.status();
        let raw_text = resp
            .text()
            .await
            .context("reading /address_info response body")?;

        if !status.is_success() {
            return Err(anyhow!(
                "/address_info returned HTTP {}: {}",
                status,
                truncate(&raw_text, 500)
            ));
        }

        let parsed: Vec<RawAddressInfo> = serde_json::from_str(&raw_text).with_context(|| {
            format!(
                "parsing /address_info response failed. Raw body was: {}",
                truncate(&raw_text, 1000)
            )
        })?;

        let Some(entry) = parsed.into_iter().next() else {
            // No entry = no UTxOs at this address (empty wallet), not an error.
            return Ok(Vec::new());
        };

        Ok(entry
            .utxo_set
            .into_iter()
            .map(|u| KoiosUtxo {
                tx_hash: u.tx_hash,
                tx_index: u.tx_index,
                address: entry.address.clone(),
                value: u.value,
                inline_datum: u.inline_datum.and_then(|d| d.bytes),
            })
            .collect())
    }

    /// Current chain tip slot, for TTL calculation (fix #8).
    pub async fn get_tip(&self) -> Result<u64> {
        let resp = self
            .client
            .get(format!("{KOIOS_BASE_URL}/tip"))
            .send()
            .await
            .context("GET /tip request failed")?;

        let status = resp.status();
        let raw_text = resp.text().await.context("reading /tip response body")?;
        if !status.is_success() {
            return Err(anyhow!(
                "/tip returned HTTP {}: {}",
                status,
                truncate(&raw_text, 500)
            ));
        }

        let parsed: Vec<RawTip> = serde_json::from_str(&raw_text)
            .with_context(|| format!("parsing /tip response failed. Raw body: {}", truncate(&raw_text, 500)))?;

        parsed
            .into_iter()
            .next()
            .and_then(|t| t.abs_slot.or(t.slot_no))
            .ok_or_else(|| anyhow!("/tip response had no abs_slot/slot_no field"))
    }

    /// Live linear-fee protocol params, for real fee calculation (fix #5).
    pub async fn get_protocol_params(&self) -> Result<ProtocolParams> {
        let resp = self
            .client
            .get(format!("{KOIOS_BASE_URL}/epoch_params"))
            .send()
            .await
            .context("GET /epoch_params request failed")?;

        let status = resp.status();
        let raw_text = resp
            .text()
            .await
            .context("reading /epoch_params response body")?;
        if !status.is_success() {
            return Err(anyhow!(
                "/epoch_params returned HTTP {}: {}",
                status,
                truncate(&raw_text, 500)
            ));
        }

        let parsed: Vec<RawEpochParams> = serde_json::from_str(&raw_text).with_context(|| {
            format!(
                "parsing /epoch_params response failed. Raw body: {}",
                truncate(&raw_text, 500)
            )
        })?;

        let entry = parsed
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("/epoch_params returned an empty array"))?;

        Ok(ProtocolParams {
            min_fee_a: entry
                .min_fee_a
                .ok_or_else(|| anyhow!("/epoch_params missing min_fee_a"))?,
            min_fee_b: entry
                .min_fee_b
                .ok_or_else(|| anyhow!("/epoch_params missing min_fee_b"))?,
        })
    }

    /// Submits raw signed tx CBOR (bytes, NOT hex — gotcha #7 from the spec).
    pub async fn submit_tx(&self, tx_cbor_hex: &str) -> Result<String> {
        let bytes = hex::decode(tx_cbor_hex).context("decoding tx_cbor_hex before submission")?;

        let resp = self
            .client
            .post(format!("{KOIOS_BASE_URL}/submittx"))
            .header("Content-Type", "application/cbor")
            .body(bytes)
            .send()
            .await
            .context("POST /submittx request failed")?;

        let status = resp.status();
        let raw_text = resp.text().await.context("reading /submittx response body")?;

        if !status.is_success() {
            // Surface the network's actual rejection reason (fee too small,
            // bad witness, expired TTL, etc.) rather than a bare status code.
            return Err(anyhow!(
                "/submittx rejected the transaction (HTTP {}): {}",
                status,
                truncate(&raw_text, 2000)
            ));
        }

        // Koios returns the tx hash as a bare quoted string on success.
        let tx_hash = raw_text.trim().trim_matches('"').to_string();
        if tx_hash.len() != 64 {
            return Err(anyhow!(
                "unexpected /submittx success response (expected a 64-char tx hash): {}",
                truncate(&raw_text, 500)
            ));
        }
        Ok(tx_hash)
    }
}

// ---------------------------------------------------------------------------
// Public types tx_builder.rs expects
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct KoiosUtxo {
    pub tx_hash: String,
    pub tx_index: u64,
    pub address: String,
    pub value: String,
    /// Hex-encoded CBOR of the inline datum, if any — already unwrapped from
    /// Koios' `{ "bytes": ..., "value": ... }` object (see bug note above).
    pub inline_datum: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProtocolParams {
    pub min_fee_a: u64,
    pub min_fee_b: u64,
}

// ---------------------------------------------------------------------------
// Raw Koios response shapes (serde), decoupled from KoiosUtxo/ProtocolParams
// so a Koios field rename doesn't ripple through tx_builder.rs.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawAddressInfo {
    #[serde(default)]
    address: String,
    #[serde(default)]
    utxo_set: Vec<RawUtxo>,
}

#[derive(Debug, Deserialize)]
struct RawUtxo {
    tx_hash: String,
    tx_index: u64,
    #[serde(default)]
    value: String,
    #[serde(default)]
    inline_datum: Option<RawInlineDatum>,
}

#[derive(Debug, Deserialize)]
struct RawInlineDatum {
    /// The actual hex CBOR — this is the field that was being missed before.
    #[serde(default)]
    bytes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawTip {
    #[serde(default)]
    abs_slot: Option<u64>,
    #[serde(default)]
    slot_no: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawEpochParams {
    #[serde(default)]
    min_fee_a: Option<u64>,
    #[serde(default)]
    min_fee_b: Option<u64>,
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}... [truncated, {} bytes total]", &s[..max_len], s.len())
    }
}
