//! JSON → PlutusData converter + collateral tx builder.
//! Converts the DealDatum JSON into proper PlutusData CBOR for the Aiken validator.

use anyhow::{anyhow, Result};
use pallas::codec::minicbor;
use pallas::codec::utils::Int;
use pallas::ledger::primitives::alonzo::{BigInt, BoundedBytes, Constr, PlutusData};
use serde_json::Value;

/// Convert a DealDatum JSON value into PlutusData CBOR hex.
pub fn deal_datum_to_plutus_cbor(json: &Value) -> Result<String> {
    let plutus_data = deal_datum_to_plutus(json)?;
    let mut buf = Vec::new();
    let mut encoder = minicbor::Encoder::new(&mut buf);
    encoder.encode(plutus_data)?;
    Ok(hex::encode(&buf))
}

/// Recursively convert a JSON value into PlutusData.
fn json_to_plutus(json: &Value) -> Result<PlutusData> {
    match json {
        Value::String(s) => {
            let bytes = if !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit()) && s.len() % 2 == 0 {
                hex::decode(s).unwrap_or_else(|_| s.as_bytes().to_vec())
            } else {
                s.as_bytes().to_vec()
            };
            Ok(PlutusData::BoundedBytes(BoundedBytes::from(bytes)))
        }
        Value::Number(n) => {
            let i = n.as_i64().ok_or_else(|| anyhow!("number doesn't fit i64"))?;
            Ok(PlutusData::BigInt(BigInt::Int(Int::from(i))))
        }
        Value::Bool(b) => {
            let tag = if *b { 0u64 } else { 1u64 };
            Ok(PlutusData::Constr(Constr { tag, any_constructor: None, fields: Vec::new() }))
        }
        Value::Array(arr) => {
            let items: Vec<PlutusData> = arr.iter().map(json_to_plutus).collect::<Result<_>>()?;
            Ok(PlutusData::Array(items))
        }
        Value::Object(obj) => {
            let keys: Vec<&String> = obj.keys().collect();
            if keys.len() == 1 {
                let key = keys[0];
                if let Some(idx) = enum_variant_index(key) {
                    let inner = &obj[key];
                    let fields = match inner {
                        Value::Object(fields_obj) => {
                            fields_obj.iter().map(|(_, v)| json_to_plutus(v)).collect::<Result<Vec<_>>>()?
                        }
                        Value::Array(arr) => {
                            arr.iter().map(json_to_plutus).collect::<Result<Vec<_>>>()?
                        }
                        _ => Vec::new(),
                    };
                    return Ok(PlutusData::Constr(Constr { tag: idx, any_constructor: None, fields }));
                }
            }
            // Struct → Constr(0, [fields in declaration order])
            let fields: Vec<PlutusData> = obj.iter().map(|(_, v)| json_to_plutus(v)).collect::<Result<_>>()?;
            Ok(PlutusData::Constr(Constr { tag: 0, any_constructor: None, fields }))
        }
        Value::Null => Ok(PlutusData::BoundedBytes(BoundedBytes::from(Vec::new()))),
    }
}

/// Build the full DealDatum as PlutusData from the JSON.
fn deal_datum_to_plutus(json: &Value) -> Result<PlutusData> {
    let obj = json.as_object().ok_or_else(|| anyhow!("DealDatum is not a JSON object"))?;

    let fields = vec![
        json_to_plutus(get_field(obj, "deal_id"))?,
        json_to_plutus(get_field(obj, "parties"))?,
        json_to_plutus(get_field(obj, "total_value"))?,
        json_to_plutus(get_field(obj, "release_units"))?,
        json_to_plutus(get_field(obj, "release_condition"))?,
        json_to_plutus(get_field(obj, "document_hash"))?,
        json_to_plutus(get_field(obj, "attachment_hashes"))?,
        json_to_plutus(get_field(obj, "dispute_window"))?,
        json_to_plutus(get_field(obj, "funding_deadline"))?,
        json_to_plutus(get_field(obj, "funded_so_far"))?,
        json_to_plutus(get_field(obj, "status"))?,
        json_to_plutus(get_field(obj, "created_at"))?,
    ];

    Ok(PlutusData::Constr(Constr { tag: 0, any_constructor: None, fields }))
}

fn get_field<'a>(obj: &'a serde_json::Map<String, Value>, key: &str) -> &'a Value {
    obj.get(key).unwrap_or(&Value::Null)
}

fn enum_variant_index(key: &str) -> Option<u64> {
    match key {
        "NoCondition" => Some(0), "ApprovalRequired" => Some(1), "ProofRequired" => Some(2),
        "TimeGated" => Some(3), "CycleGated" => Some(4),
        "MutualConfirm" => Some(0), "OracleConfirm" => Some(1), "TimeoutDispute" => Some(2),
        "HybridArbiter" => Some(3), "TimeVesting" => Some(4), "RecurringSubscription" => Some(5),
        "Deposit" => Some(0), "ClaimUnit" => Some(1), "SubmitProof" => Some(2),
        "ReviewProof" => Some(3), "RaiseDispute" => Some(4), "ArbiterResolve" => Some(5),
        "Refund" => Some(6),
        _ => None,
    }
}

// ============================================================
// Collateral — real on-chain transaction via Pallas
// ============================================================

pub async fn build_collateral_tx(
    koios: &crate::modules::koios::KoiosProvider,
    user_address: &str,
    script_address: &str,
    lovelace_amount: u64,
    agreement_id: &str,
) -> Result<String> {
    use crate::modules::tx_builder as tb;

    let datum = PlutusData::Constr(Constr {
        tag: 0,
        any_constructor: None,
        fields: vec![
            PlutusData::BoundedBytes(BoundedBytes::from(agreement_id.as_bytes().to_vec())),
            PlutusData::BoundedBytes(BoundedBytes::from(user_address.as_bytes().to_vec())),
            PlutusData::BigInt(BigInt::Int(Int::from(lovelace_amount as i64))),
        ],
    });

    let mut datum_buf = Vec::new();
    let mut encoder = minicbor::Encoder::new(&mut datum_buf);
    encoder.encode(datum)?;
    let datum_cbor_hex = hex::encode(&datum_buf);

    tb::build_lock_tx(koios, user_address, script_address, lovelace_amount, &datum_cbor_hex).await
}

pub async fn submit_collateral_tx(unsigned_tx_cbor: &str, witness_cbor: &str) -> Result<String> {
    use crate::modules::tx_builder as tb;
    use crate::modules::koios::KoiosProvider;

    let signed_cbor = tb::assemble_signed_tx(unsigned_tx_cbor, witness_cbor)?;
    let koios = KoiosProvider::new();
    koios.submit_tx(&signed_cbor).await
}
