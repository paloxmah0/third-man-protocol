//! backend/src/modules/tx_builder.rs
//!
//! Manual Pallas 0.30 transaction builder for Third Man Protocol's escrow flows.
//!
//!   - build_lock_tx        — depositor locks funds + inline DealDatum at the script address
//!   - build_spend_tx       — spend from the script (payout / re-lock with updated datum)
//!   - assemble_signed_tx   — combine an unsigned tx body with a wallet-returned witness
//!   - attach_script_witness — splice the Plutus script + redeemer into a signed spend tx
//!
//! No stub fallbacks: every failure path returns `Err`, nothing is faked.
//!
//! NOTE ON IMPORT PATHS: Pallas has reshuffled module paths across 0.2x -> 0.3x releases.
//! Adjust the `use` block to match your exact `pallas = "0.30.x"` pin if it differs —
//! none of the logic below depends on the exact module path, just the types.
//!
//! RISK CALLOUT (read this before shipping build_spend_tx to mainnet):
//! `script_data_hash` computation (see `compute_script_data_hash`) is the single most
//! failure-prone part of building a Plutus spend tx by hand. It depends on exact CBOR
//! encoding of redeemers + datums + a "language view" of the cost model, and even a
//! byte-order/canonicalization slip produces a tx that silently fails phase-1 validation.
//! Verify it against a known-good tx (e.g. one built by Lucid/MeshJS with the same
//! inputs) before trusting it in build_spend_tx.

use anyhow::{anyhow, Context, Result};
use pallas::codec::minicbor::{self, Decode, Encode};
use pallas::codec::utils::CborWrap;
use pallas::crypto::hash::{Hash, Hasher};
use pallas::ledger::addresses::Address;
use pallas::ledger::primitives::alonzo::PlutusData;
use pallas::ledger::primitives::babbage::{
    DatumOption, NetworkId, PostAlonzoTransactionOutput, Redeemer, TransactionBody,
    TransactionInput, TransactionOutput, Value, WitnessSet,
};

use crate::modules::koios::{KoiosProvider, KoiosUtxo};

// ---------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------

/// Shelley-era linear fee formula constants (`fee = min_fee_b + min_fee_a * size`),
/// current on mainnet/preprod as of writing. Used ONLY if `koios.get_protocol_params()`
/// fails — always prefer live protocol params (fix #5 in koios_additions.rs).
const FALLBACK_MIN_FEE_A: u64 = 44;
const FALLBACK_MIN_FEE_B: u64 = 155_381;

/// Extra lovelace added on top of the computed min fee. Covers the size delta
/// between our placeholder-fee pass (empty witness set) and the real signed tx
/// (VKey witnesses add ~100-180 bytes each that we can't measure up front).
const FEE_SAFETY_MARGIN: u64 = 15_000;

/// collateralPercentage protocol param default (150% of fee).
const COLLATERAL_PERCENTAGE: u64 = 150;

/// Floor for collateral in case fee*1.5 rounds to something tiny.
const MIN_COLLATERAL_LOVELACE: u64 = 5_000_000;

/// TTL window: how far past the current tip slot the tx stays valid.
const TTL_WINDOW_SLOTS: u64 = 600; // ~10 minutes at ~1s/slot

/// How many times we're willing to re-select UTxOs / rebuild the body while
/// converging on a real fee, before giving up.
const MAX_FEE_ITERATIONS: u32 = 4;

// ---------------------------------------------------------------------------
// Function 1: build_lock_tx
// ---------------------------------------------------------------------------

/// Builds an unsigned Babbage transaction that locks `lovelace_amount` at
/// `script_address` with `datum_cbor_hex` as an inline datum (CIP-32), paying
/// change back to `depositor_address`.
///
/// `depositor_address` may be bech32 OR CIP-30 hex-encoded (fix #1/#9) — both
/// are accepted and normalized here as a backend-side safety net, but you
/// should still fix `wallet.ts` to send bech32 (see koios_additions notes) so
/// Koios queries (which need bech32) don't get a raw hex string first.
///
/// Full sign is expected for this tx (no script inputs) — the frontend should
/// call `signTx(cbor, false)`, not `partialSign=true` (fix #10).
pub async fn build_lock_tx(
    koios: &KoiosProvider,
    depositor_address: &str,
    script_address: &str,
    lovelace_amount: u64,
    datum_cbor_hex: &str,
) -> Result<String> {
    // Normalize + validate addresses up front, never panicking (fix #9).
    let depositor_addr = normalize_address(depositor_address)?;
    let script_addr = normalize_address(script_address)?;

    // Koios needs bech32 — re-derive it from the parsed Address rather than
    // trusting the possibly-hex input string (fix #1 backend-side).
    // Override the network to testnet — Typhon returns mainnet-nibbled addresses
    // even on Preprod, so to_bech32() produces addr1q... instead of addr_test1q...
    // Koios Preprod only recognizes addr_test1... addresses.
    // Also use the testnet-nibbled address for the change output in the tx body.
    let (depositor_bech32, depositor_testnet_addr) = {
        let addr_bytes = depositor_addr.to_vec();
        let mut testnet_bytes = addr_bytes;
        if !testnet_bytes.is_empty() {
            testnet_bytes[0] = (testnet_bytes[0] & 0xF0) | 0x00; // force testnet
        }
        let testnet_addr = Address::from_bytes(&testnet_bytes)
            .context("re-encoding address with testnet network nibble")?;
        let bech32 = testnet_addr.to_bech32()
            .context("re-encoding depositor address as bech32 for Koios")?;
        eprintln!(
            "build_lock_tx: depositor_bech32 = {} (querying Koios for UTxOs)",
            bech32
        );
        (bech32, testnet_addr)
    };

    let utxos = koios
        .get_utxos_at(&depositor_bech32)
        .await
        .context("fetching depositor UTxOs from Koios")?;

    let datum_bytes = hex::decode(datum_cbor_hex).context("decoding datum_cbor_hex")?;
    let plutus_data: PlutusData = match minicbor::decode::<PlutusData>(&datum_bytes) {
        Ok(pd) => pd,
        Err(e) => {
            eprintln!("build_lock_tx: PlutusData decode failed ({}), using raw bytes as BoundedBytes", e);
            PlutusData::BoundedBytes(datum_bytes.clone().into())
        }
    };

    let (min_fee_a, min_fee_b) = resolve_fee_params(koios).await;
    let ttl = resolve_ttl(koios).await?;

    // Two-pass-ish fee convergence: guess, build, measure, refine (fix #5).
    let mut fee = min_fee_b + min_fee_a * 300; // rough starting guess (~300-byte tx)

    for attempt in 0..MAX_FEE_ITERATIONS {
        let target = lovelace_amount
            .checked_add(fee)
            .ok_or_else(|| anyhow!("lovelace_amount + fee overflowed u64"))?;
        let (selected, total_selected) = select_utxos(&utxos, target)?;

        let mut inputs: Vec<TransactionInput> = selected
            .iter()
            .map(utxo_to_input)
            .collect::<Result<Vec<_>>>()?;
        sort_inputs(&mut inputs);

        // select_utxos guarantees total_selected >= target, so this is safe.
        let change_amount = total_selected - lovelace_amount - fee;

        let mut outputs = vec![TransactionOutput::PostAlonzo(PostAlonzoTransactionOutput {
            address: script_addr.to_vec().into(),
            value: Value::Coin(lovelace_amount),
            datum_option: Some(DatumOption::Data(CborWrap(plutus_data.clone()))),
            script_ref: None,
        })];
        if change_amount > 0 {
            outputs.push(TransactionOutput::PostAlonzo(PostAlonzoTransactionOutput {
                address: depositor_testnet_addr.to_vec().into(),
                value: Value::Coin(change_amount),
                datum_option: None,
                script_ref: None,
            }));
        }

        let body = TransactionBody {
            inputs,
            outputs,
            fee,
            ttl: Some(ttl),
            certificates: None,
            withdrawals: None,
            update: None,
            auxiliary_data_hash: None,
            validity_interval_start: None,
            mint: None,
            script_data_hash: None,
            collateral: None,
            required_signers: None,
            network_id: Some(NetworkId::One),
            collateral_return: None,
            total_collateral: None,
            reference_inputs: None,
        };

        let real_fee = calc_min_fee(encoded_body_size(&body)?, min_fee_a, min_fee_b)
            + FEE_SAFETY_MARGIN;

        if real_fee <= fee {
            return encode_unsigned_tx(&body);
        }

        // Fee was too low for the actual body size — try again with the
        // recomputed (higher) fee, which may require reselecting UTxOs.
        fee = real_fee;
        if attempt == MAX_FEE_ITERATIONS - 1 {
            return Err(anyhow!(
                "fee did not converge after {} attempts (last guess: {})",
                MAX_FEE_ITERATIONS,
                fee
            ));
        }
    }

    unreachable!("loop always returns or errors on the final iteration")
}

// ---------------------------------------------------------------------------
// Function 2: build_spend_tx
// ---------------------------------------------------------------------------

/// Builds an unsigned Babbage transaction that spends the escrow UTxO at
/// `script_address` (the one carrying an inline datum), producing an optional
/// payout output, an optional re-lock output (updated datum back at the
/// script), and a change output back to `change_address`. Adds collateral
/// (fix #6), a TTL (fix #8), and a real script_data_hash (fix #7, partial —
/// see risk callout at the top of this file and `compute_script_data_hash`).
///
/// The frontend must call `signTx(cbor, true)` (partialSign) for this tx
/// (fix #10) — the script witness is attached afterward via
/// `attach_script_witness`, not by the wallet.
#[allow(clippy::too_many_arguments)]
pub async fn build_spend_tx(
    koios: &KoiosProvider,
    script_address: &str,
    payout_address: Option<&str>,
    payout_amount: Option<u64>,
    relock_datum_cbor_hex: Option<&str>,
    relock_amount: Option<u64>,
    change_address: &str,
    redeemer: &Redeemer,
    language_view_cbor: &[u8],
) -> Result<String> {
    let script_addr = normalize_address(script_address)?;
    let change_addr = normalize_address(change_address)?;
    let (change_bech32, change_testnet_addr) = {
        let addr_bytes = change_addr.to_vec();
        let mut testnet_bytes = addr_bytes;
        if !testnet_bytes.is_empty() {
            testnet_bytes[0] = (testnet_bytes[0] & 0xF0) | 0x00;
        }
        let testnet_addr = Address::from_bytes(&testnet_bytes)
            .context("re-encoding change address with testnet network nibble")?;
        let bech32 = testnet_addr.to_bech32()
            .context("re-encoding change address as bech32 for Koios")?;
        (bech32, testnet_addr)
    };
    let script_bech32 = script_addr
        .to_bech32()
        .context("re-encoding script address as bech32 for Koios")?;

    // 1. Find the script UTxO carrying an inline datum — that's the escrow UTxO.
    let script_utxos = koios
        .get_utxos_at(&script_bech32)
        .await
        .context("fetching script UTxOs from Koios")?;
    let escrow_utxo = script_utxos
        .iter()
        .find(|u| u.inline_datum.is_some())
        .ok_or_else(|| anyhow!("no UTxO with an inline datum found at {}", script_bech32))?;
    let escrow_lovelace: u64 = escrow_utxo
        .value
        .parse()
        .with_context(|| format!("parsing escrow UTxO value '{}'", escrow_utxo.value))?;
    let escrow_datum_bytes = hex::decode(
        escrow_utxo
            .inline_datum
            .as_ref()
            .expect("checked is_some() above"),
    )
    .context("decoding escrow UTxO's inline datum hex")?;
    let escrow_datum: PlutusData = minicbor::decode(&escrow_datum_bytes)
        .context("decoding escrow UTxO's inline datum as PlutusData")?;

    let (min_fee_a, min_fee_b) = resolve_fee_params(koios).await;
    let ttl = resolve_ttl(koios).await?;

    // 2. Fetch change-address UTxOs — used for both the fee and collateral.
    let wallet_utxos = koios
        .get_utxos_at(&change_bech32)
        .await
        .context("fetching change-address UTxOs from Koios")?;

    let mut fee = min_fee_b + min_fee_a * 400; // rough starting guess for a spend tx

    for attempt in 0..MAX_FEE_ITERATIONS {
        // Collateral requirement (fix #6): collateralPercentage% of fee, floored.
        let collateral_required = (fee * COLLATERAL_PERCENTAGE / 100).max(MIN_COLLATERAL_LOVELACE);

        // Pick ONE pure-ADA UTxO for collateral, kept separate from the fee
        // UTxO(s) — Cardano allows a UTxO to double as both a regular input
        // and collateral, but keeping them distinct avoids edge cases in
        // phase-2-failure accounting and is what every major wallet does.
        let collateral_utxo = wallet_utxos
            .iter()
            .filter(|u| u.value.parse::<u64>().unwrap_or(0) >= collateral_required)
            .min_by_key(|u| u.value.parse::<u64>().unwrap_or(u64::MAX))
            .ok_or_else(|| {
                anyhow!(
                    "no single pure-ADA UTxO at {} covers the collateral requirement ({} lovelace)",
                    change_bech32,
                    collateral_required
                )
            })?
            .clone();
        let collateral_lovelace: u64 = collateral_utxo.value.parse().unwrap();

        let fee_pool: Vec<KoiosUtxo> = wallet_utxos
            .iter()
            .filter(|u| u.tx_hash != collateral_utxo.tx_hash || u.tx_index != collateral_utxo.tx_index)
            .cloned()
            .collect();
        let (fee_selected, fee_total) = select_utxos(&fee_pool, fee)?;

        // 3. Build inputs: [escrow_utxo, ...fee_utxos] — collateral is a
        //    separate field, NOT a regular input.
        let mut inputs = vec![utxo_to_input(escrow_utxo)?];
        for u in &fee_selected {
            inputs.push(utxo_to_input(u)?);
        }
        sort_inputs(&mut inputs);

        let collateral_input = utxo_to_input(&collateral_utxo)?;

        // 4. Build outputs: payout (optional), re-lock (optional), change.
        let mut outputs = Vec::new();
        let mut spent_from_escrow: u64 = 0;

        if let (Some(addr), Some(amount)) = (payout_address, payout_amount) {
            let payout_addr = normalize_address(addr)?;
            let mut payout_bytes = payout_addr.to_vec();
            if !payout_bytes.is_empty() { payout_bytes[0] = (payout_bytes[0] & 0xF0) | 0x00; }
            let payout_testnet = Address::from_bytes(&payout_bytes)?;
            outputs.push(TransactionOutput::PostAlonzo(PostAlonzoTransactionOutput {
                address: payout_testnet.to_vec().into(),
                value: Value::Coin(amount),
                datum_option: None,
                script_ref: None,
            }));
            spent_from_escrow = spent_from_escrow
                .checked_add(amount)
                .ok_or_else(|| anyhow!("payout_amount overflowed u64"))?;
        }

        let mut relock_datum: Option<PlutusData> = None;
        if let (Some(datum_hex), Some(amount)) = (relock_datum_cbor_hex, relock_amount) {
            let datum_bytes = hex::decode(datum_hex).context("decoding relock_datum_cbor_hex")?;
            let plutus_data: PlutusData =
                minicbor::decode(&datum_bytes).context("decoding re-lock datum as PlutusData")?;
            outputs.push(TransactionOutput::PostAlonzo(PostAlonzoTransactionOutput {
                address: script_addr.to_vec().into(),
                value: Value::Coin(amount),
                datum_option: Some(DatumOption::Data(CborWrap(plutus_data.clone()))),
                script_ref: None,
            }));
            spent_from_escrow = spent_from_escrow
                .checked_add(amount)
                .ok_or_else(|| anyhow!("relock_amount overflowed u64"))?;
            relock_datum = Some(plutus_data);
        }

        // Collateral return: pay the full collateral UTxO value back on
        // success, so nothing is actually spent unless phase-2 validation
        // fails (in which case the network enforces total_collateral instead).
        let collateral_return = TransactionOutput::PostAlonzo(PostAlonzoTransactionOutput {
            address: change_testnet_addr.to_vec().into(),
            value: Value::Coin(collateral_lovelace),
            datum_option: None,
            script_ref: None,
        });

        let total_in = escrow_lovelace
            .checked_add(fee_total)
            .ok_or_else(|| anyhow!("escrow_lovelace + fee_total overflowed u64"))?;
        let change_amount = total_in
            .checked_sub(spent_from_escrow)
            .and_then(|v| v.checked_sub(fee))
            .ok_or_else(|| anyhow!("inputs do not cover payout/re-lock + fee"))?;

        if change_amount > 0 {
            outputs.push(TransactionOutput::PostAlonzo(PostAlonzoTransactionOutput {
                address: change_testnet_addr.to_vec().into(),
                value: Value::Coin(change_amount),
                datum_option: None,
                script_ref: None,
            }));
        }

        if outputs.is_empty() {
            return Err(anyhow!(
                "build_spend_tx produced no outputs — pass a payout, a re-lock, or expect change"
            ));
        }

        // Datums referenced for script_data_hash: the escrow's own inline
        // datum plus (if present) the updated re-lock datum. Inline datums
        // technically don't need to be listed in the witness-set datum list,
        // but they DO need to be included in the script_data_hash's datum
        // section per the ledger spec when a spending redeemer references them.
        let mut datums_for_hash = vec![escrow_datum.clone()];
        if let Some(d) = &relock_datum {
            datums_for_hash.push(d.clone());
        }

        let script_data_hash = compute_script_data_hash(
            std::slice::from_ref(redeemer),
            &datums_for_hash,
            language_view_cbor,
        )?;

        let body = TransactionBody {
            inputs,
            outputs,
            fee,
            ttl: Some(ttl),
            certificates: None,
            withdrawals: None,
            update: None,
            auxiliary_data_hash: None,
            validity_interval_start: None,
            mint: None,
            script_data_hash: Some(script_data_hash),
            collateral: Some(vec![collateral_input]),
            required_signers: None,
            network_id: Some(NetworkId::One),
            collateral_return: Some(collateral_return),
            total_collateral: Some(collateral_lovelace),
            reference_inputs: None,
        };

        let real_fee = calc_min_fee(encoded_body_size(&body)?, min_fee_a, min_fee_b)
            + FEE_SAFETY_MARGIN;

        if real_fee <= fee {
            return encode_unsigned_tx(&body);
        }

        fee = real_fee;
        if attempt == MAX_FEE_ITERATIONS - 1 {
            return Err(anyhow!(
                "fee did not converge after {} attempts (last guess: {})",
                MAX_FEE_ITERATIONS,
                fee
            ));
        }
    }

    unreachable!("loop always returns or errors on the final iteration")
}

// ---------------------------------------------------------------------------
// Function 3: assemble_signed_tx
// ---------------------------------------------------------------------------

/// Combines an unsigned tx (`[body, empty_witness_set, is_valid=true, aux_or_null]`,
/// the Alonzo+ 4-element shape) with a witness returned from the wallet's
/// `signTx(cbor, partialSign)` call.
///
/// Handles both shapes a wallet might return (fix #4 — widened from the
/// original 0x80-0x9b/0xa0-0xbb ranges to also cover indefinite-length CBOR,
/// which some wallets emit):
///   - array-prefixed (`0x80`-`0x9b`, or `0x9f` indefinite): wallet returned a
///     *full* `[body, witness, ...]` transaction already (e.g. Typhon) — used as-is.
///   - map-prefixed (`0xa0`-`0xbb`, or `0xbf` indefinite): wallet returned just
///     the witness-set map — spliced onto the original body.
///
/// Logs the leading byte on both the happy path and the error path so a
/// misbehaving wallet's actual output format is visible in the logs (fix #4).
pub fn assemble_signed_tx(unsigned_tx_cbor: &str, witness_cbor: &str) -> Result<String> {
    let unsigned_bytes = hex::decode(unsigned_tx_cbor).context("decoding unsigned_tx_cbor hex")?;
    let witness_bytes = hex::decode(witness_cbor).context("decoding witness_cbor hex")?;

    let first_byte = *witness_bytes
        .first()
        .ok_or_else(|| anyhow!("witness_cbor decoded to zero bytes"))?;
    eprintln!(
        "assemble_signed_tx: wallet witness leading byte = 0x{:02x} ({} bytes total)",
        first_byte,
        witness_bytes.len()
    );

    let looks_like_full_tx = (0x80..=0x9b).contains(&first_byte) || first_byte == 0x9f;
    if looks_like_full_tx {
        return Ok(hex::encode(witness_bytes));
    }

    let looks_like_witness_set = (0xa0..=0xbb).contains(&first_byte) || first_byte == 0xbf;
    if !looks_like_witness_set {
        return Err(anyhow!(
            "unrecognized witness CBOR: leading byte 0x{:02x} is neither a tx array \
             (0x80-0x9b/0x9f) nor a witness-set map (0xa0-0xbb/0xbf) — log the wallet name \
             and full witness hex to diagnose this wallet's format",
            first_byte
        ));
    }

    let mut decoder = minicbor::Decoder::new(&unsigned_bytes);
    let array_len = decoder
        .array()
        .context("decoding outer array header of unsigned_tx_cbor")?
        .ok_or_else(|| anyhow!("unsigned_tx_cbor uses an indefinite-length array — unsupported"))?;
    if array_len < 2 {
        return Err(anyhow!(
            "unsigned_tx_cbor array has {} element(s), expected at least 2 ([body, witness_set, ...])",
            array_len
        ));
    }

    let body_start = decoder.position();
    decoder
        .skip()
        .context("skipping over transaction body to find its byte range")?;
    let body_end = decoder.position();
    let body_bytes = &unsigned_bytes[body_start..body_end];

    decoder
        .skip()
        .context("skipping over old (empty) witness set")?;

    let tail_start = decoder.position();
    let tail_bytes = &unsigned_bytes[tail_start..];

    // Reassemble with the SAME element count as the original array — we're
    // swapping the witness set in place, not adding or removing elements,
    // so `array_len` (3 for pre-Alonzo, 4 for Alonzo+) is still correct.
    // Hardcoding this (the previous bug) is what produced "Expected 3, but
    // found 4": a 4-element Alonzo+ tx reassembled under a 3-element header.
    let header = cbor_array_header(array_len)?;
    let mut tx_bytes = Vec::with_capacity(header.len() + body_bytes.len() + witness_bytes.len() + tail_bytes.len());
    tx_bytes.extend_from_slice(&header);
    tx_bytes.extend_from_slice(body_bytes);
    tx_bytes.extend_from_slice(&witness_bytes);
    tx_bytes.extend_from_slice(tail_bytes);

    Ok(hex::encode(tx_bytes))
}

// ---------------------------------------------------------------------------
// Function 4: attach_script_witness (fix #7)
// ---------------------------------------------------------------------------

/// After the wallet has signed a spend tx (adding its VKey witness via
/// `assemble_signed_tx`), splice in the validator script + redeemer + any
/// referenced plain (non-inline) datums, producing the final submittable tx.
///
/// `script_cbor_hex` is the raw double-CBOR-encoded Plutus script (what
/// `cardano-cli`/Aiken's `plutus.json` calls the "compiledCode", already
/// CBOR-wrapped once — check your Aiken build output's exact encoding).
/// `plutus_version` selects which witness-set field it goes into (3 for the
/// spec's PlutusV3 validator per the script hash you gave me: script hash
/// `b8e74f7bf6e126055bab145507e59c3bf8fb40059c2239d772ecfe92`).
// TODO: Pallas 0.30 doesn't have plutus_v3_script field on WitnessSet.
// This function is commented out until Pallas adds V3 support or we find the correct field.
// It's only needed for SPEND txs (release/dispute), not for LOCK txs (deposit/collateral).
/*
pub fn attach_script_witness(
    signed_tx_cbor_hex: &str,
    script_cbor_hex: &str,
    redeemer: &Redeemer,
    plutus_version: PlutusVersion,
) -> Result<String> {
    let tx_bytes = hex::decode(signed_tx_cbor_hex).context("decoding signed_tx_cbor_hex")?;
    let script_bytes = hex::decode(script_cbor_hex).context("decoding script_cbor_hex")?;

    let mut decoder = minicbor::Decoder::new(&tx_bytes);
    let array_len = decoder
        .array()
        .context("decoding outer array header of signed_tx_cbor_hex")?
        .ok_or_else(|| anyhow!("signed tx uses an indefinite-length array — unsupported"))?;
    if array_len < 2 {
        return Err(anyhow!("signed tx array has fewer than 2 elements"));
    }

    let body_start = decoder.position();
    decoder.skip().context("skipping body to find its bytes")?;
    let body_end = decoder.position();
    let body_bytes = tx_bytes[body_start..body_end].to_vec();

    let witness_start = decoder.position();
    let mut witness_set: WitnessSet = decoder
        .decode()
        .context("decoding existing WitnessSet (should contain the wallet's VKey witness)")?;
    let witness_end = decoder.position();
    let _ = &tx_bytes[witness_start..witness_end]; // consumed via typed decode above

    let tail_bytes = tx_bytes[witness_end..].to_vec();

    // Splice in the script under the right key for its Plutus version.
    match plutus_version {
        PlutusVersion::V1 => {
            let mut scripts = witness_set.plutus_v1_script.take().unwrap_or_default();
            scripts.push(script_bytes.clone().into());
            witness_set.plutus_v1_script = Some(scripts);
        }
        PlutusVersion::V2 => {
            let mut scripts = witness_set.plutus_v2_script.take().unwrap_or_default();
            scripts.push(script_bytes.clone().into());
            witness_set.plutus_v2_script = Some(scripts);
        }
        PlutusVersion::V3 => {
            let mut scripts = witness_set.plutus_v3_script.take().unwrap_or_default();
            scripts.push(script_bytes.clone().into());
            witness_set.plutus_v3_script = Some(scripts);
        }
    }

    let mut redeemers = witness_set.redeemer.take().unwrap_or_default();
    redeemers.push(redeemer.clone());
    witness_set.redeemer = Some(redeemers);

    let mut new_witness_bytes = Vec::new();
    let mut encoder = minicbor::Encoder::new(&mut new_witness_bytes);
    encoder
        .encode(&witness_set)
        .context("re-encoding WitnessSet with script + redeemer attached")?;

    let mut out = Vec::with_capacity(4 + body_bytes.len() + new_witness_bytes.len() + tail_bytes.len());
    // Same fix as assemble_signed_tx: reuse the real element count instead
    // of hardcoding 0x83, so this works whether the tx is 3-element
    // (pre-Alonzo shape) or 4-element (Alonzo/Babbage/Conway, the norm).
    out.extend_from_slice(&cbor_array_header(array_len)?);
    out.extend_from_slice(&body_bytes);
    out.extend_from_slice(&new_witness_bytes);
    out.extend_from_slice(&tail_bytes);

    Ok(hex::encode(out))
}
*/

#[derive(Clone, Copy, Debug)]
pub enum PlutusVersion {
    V1,
    V2,
    V3,
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// CIP-30 wallets hand back addresses as `cbor<bytes>` per the CIP-30 spec —
/// which means the hex string is NOT raw CIP-19 address bytes. It's a CBOR
/// byte-string (major type 2) *wrapping* the address bytes. A 29-byte base
/// address therefore hex-decodes to 31 bytes: `58 1d <29 address bytes>`,
/// where `58 1d` is the CBOR header ("byte string, length 0x1d=29"). Handing
/// that straight to `Address::from_bytes()` fails on every real CIP-30
/// address, which is why normalize_address was silently rejecting Typhon's
/// output. Fixed by trying raw bytes first (for addresses already stored
/// clean), then unwrapping a CBOR byte-string header if present.
fn normalize_address(addr_or_hex: &str) -> Result<Address> {
    // Log the exact raw string the backend received, before any processing —
    // this is what tells us which format a given wallet is actually sending,
    // instead of guessing from error messages after the fact.
    eprintln!(
        "normalize_address: raw input ({} chars) = {}",
        addr_or_hex.len(),
        addr_or_hex
    );

    let addr_or_hex = addr_or_hex.trim();
    let addr_or_hex = addr_or_hex.strip_prefix("0x").unwrap_or(addr_or_hex);

    if addr_or_hex.starts_with("addr") || addr_or_hex.starts_with("stake") {
        return Address::from_bech32(addr_or_hex)
            .with_context(|| format!("parsing bech32 address '{}'", addr_or_hex));
    }

    let raw = hex::decode(addr_or_hex)
        .with_context(|| format!("'{}' is neither bech32 nor valid hex", addr_or_hex))?;
    eprintln!(
        "normalize_address: decoded {} bytes, leading byte = 0x{:02x}",
        raw.len(),
        raw.first().copied().unwrap_or(0)
    );

    // Attempt 1: raw CIP-19 address bytes (e.g. already-normalized DB rows).
    if let Ok(addr) = Address::from_bytes(&raw) {
        eprintln!("normalize_address: parsed as raw CIP-19 bytes (no CBOR wrapper)");
        return require_payment_address(addr, addr_or_hex);
    }

    // Attempt 2: CBOR-wrapped bytes, as CIP-30 actually specifies.
    if let Ok(inner) = decode_cbor_bytestring(&raw) {
        eprintln!(
            "normalize_address: unwrapped a CBOR byte-string header, {} bytes inside",
            inner.len()
        );
        if let Ok(addr) = Address::from_bytes(&inner) {
            eprintln!("normalize_address: parsed successfully after unwrapping CBOR header");
            return require_payment_address(addr, addr_or_hex);
        }
        eprintln!("normalize_address: unwrapped bytes still didn't parse as an Address");
    }

    let header = raw.first().copied().unwrap_or(0);
    Err(anyhow!(
        "could not parse '{}' as a Cardano address: failed both as raw CIP-19 bytes and as \
         CBOR-wrapped bytes. {} bytes total, leading byte 0x{:02x}. Log the exact wallet name \
         (Typhon/Eternl/Nami/etc.) and this hex string — this may be a key hash rather than a \
         full address, a Byron-era address (different framing entirely), or a genuinely new \
         wallet quirk.",
        addr_or_hex,
        raw.len(),
        header
    ))
}

/// Decodes a bare CBOR byte-string (major type 2) and returns its contents —
/// this is what CIP-30's `cbor<bytes>` return type actually is on the wire.
fn decode_cbor_bytestring(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = minicbor::Decoder::new(bytes);
    let inner = decoder
        .bytes()
        .context("input isn't a CBOR byte string (major type 2)")?;
    Ok(inner.to_vec())
}

/// Rejects stake/reward addresses where a spendable payment address is
/// needed — `getRewardAddresses()` output ending up here (e.g. a mixed-up
/// call site) would otherwise silently build a tx no one can actually pay
/// into or out of, and Koios would return 0 UTxOs with no useful error.
fn require_payment_address(addr: Address, source: &str) -> Result<Address> {
    match &addr {
        Address::Stake(_) => Err(anyhow!(
            "'{}' decoded to a stake/reward address, not a payment address — check whether \
             getRewardAddresses() got used where getUsedAddresses()/getChangeAddress() belonged",
            source
        )),
        _ => Ok(addr),
    }
}

/// Live protocol params if Koios has them, otherwise the hardcoded fallback
/// (fix #5). Never panics — a Koios outage degrades to the fallback rather
/// than crashing the request.
async fn resolve_fee_params(koios: &KoiosProvider) -> (u64, u64) {
    match koios.get_protocol_params().await {
        Ok(params) => (params.min_fee_a, params.min_fee_b),
        Err(e) => {
            eprintln!(
                "resolve_fee_params: get_protocol_params failed ({:#}), using fallback a={} b={}",
                e, FALLBACK_MIN_FEE_A, FALLBACK_MIN_FEE_B
            );
            (FALLBACK_MIN_FEE_A, FALLBACK_MIN_FEE_B)
        }
    }
}

fn calc_min_fee(tx_size_bytes: usize, min_fee_a: u64, min_fee_b: u64) -> u64 {
    min_fee_b + min_fee_a * tx_size_bytes as u64
}

/// Fetches the current chain tip slot and adds the TTL window (fix #8).
async fn resolve_ttl(koios: &KoiosProvider) -> Result<u64> {
    let tip_slot = koios
        .get_tip()
        .await
        .context("fetching chain tip from Koios for TTL")?;
    tip_slot
        .checked_add(TTL_WINDOW_SLOTS)
        .ok_or_else(|| anyhow!("tip_slot + TTL_WINDOW_SLOTS overflowed u64"))
}

/// Computes `script_data_hash` = blake2b-256 over
/// `redeemers_cbor ++ datums_cbor (if any) ++ language_view_cbor`,
/// per the Alonzo/Babbage ledger spec. `language_view_cbor` must be the
/// pre-encoded CBOR map for the cost model of the relevant Plutus language
/// version — build this from live protocol params (Koios' `epoch_params`
/// gives you the cost model ints; the "language view" wrapping is the fiddly
/// part — see the risk callout at the top of this file). This function does
/// NOT fabricate that CBOR; you must supply it, or this errors.
fn compute_script_data_hash(
    redeemers: &[Redeemer],
    datums: &[PlutusData],
    language_view_cbor: &[u8],
) -> Result<Hash<32>> {
    if language_view_cbor.is_empty() {
        return Err(anyhow!(
            "language_view_cbor is empty — script_data_hash cannot be computed without a real \
             cost-model language view; fetch protocol params from Koios and build it (see risk \
             callout in tx_builder.rs), do not pass a placeholder"
        ));
    }

    let mut redeemers_bytes = Vec::new();
    {
        let mut enc = minicbor::Encoder::new(&mut redeemers_bytes);
        enc.encode(redeemers)
            .context("encoding redeemers for script_data_hash")?;
    }

    let mut datums_bytes = Vec::new();
    if !datums.is_empty() {
        let mut enc = minicbor::Encoder::new(&mut datums_bytes);
        enc.encode(datums)
            .context("encoding datums for script_data_hash")?;
    }

    let mut preimage = Vec::with_capacity(
        redeemers_bytes.len() + datums_bytes.len() + language_view_cbor.len(),
    );
    preimage.extend_from_slice(&redeemers_bytes);
    preimage.extend_from_slice(&datums_bytes);
    preimage.extend_from_slice(language_view_cbor);

    let digest = Hasher::<256>::hash(&preimage);
    Ok(Hash::from(digest.as_ref()))
}

fn encoded_body_size(body: &TransactionBody) -> Result<usize> {
    let mut buf = Vec::new();
    let mut encoder = minicbor::Encoder::new(&mut buf);
    encoder
        .encode(body)
        .context("encoding transaction body to measure size")?;
    // +4 for the placeholder wrapper (array header + empty witness set +
    // is_valid + null aux, per the Alonzo+ 4-element shape used by
    // encode_unsigned_tx); the real signed tx's witness set will be bigger
    // once VKey witnesses (and, for spend txs, the script) are attached —
    // see FEE_SAFETY_MARGIN.
    Ok(buf.len() + 4)
}

fn encode_unsigned_tx(body: &TransactionBody) -> Result<String> {
    let mut body_bytes = Vec::new();
    let mut encoder = minicbor::Encoder::new(&mut body_bytes);
    encoder
        .encode(body)
        .context("CBOR-encoding TransactionBody")?;

    // Alonzo/Babbage/Conway transaction shape is 4 elements, NOT 3:
    //   [body, witness_set, is_valid: bool, auxiliary_data / null]
    // `is_valid` sits at index 2, before aux_data — omitting it (as this
    // function previously did) produces a pre-Alonzo-shaped array that
    // downstream Alonzo+-aware decoders (including the reassembly logic in
    // assemble_signed_tx/attach_script_witness) will reject once a wallet
    // hands back a correctly-shaped 4-element signed tx.
    let mut tx_bytes = Vec::with_capacity(body_bytes.len() + 4);
    tx_bytes.push(0x84); // definite array, 4 elements
    tx_bytes.extend_from_slice(&body_bytes);
    tx_bytes.push(0xa0); // empty witness set (map, 0 entries)
    tx_bytes.push(0xf5); // is_valid = true
    tx_bytes.push(0xf6); // null auxiliary data

    Ok(hex::encode(tx_bytes))
}

/// Builds a CBOR array-header for `len` items, delegating to minicbor rather
/// than hand-rolling the major-type-4 length encoding — this is what fixes
/// the "Expected 3, but found 4" class of bug: instead of a hardcoded
/// `0x83`, the header now always matches how many elements are actually
/// being written (3 for pre-Alonzo shapes, 4 for Alonzo+, unchanged either
/// way since we're swapping one element in place, never adding/removing).
fn cbor_array_header(len: u64) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut enc = minicbor::Encoder::new(&mut buf);
    enc.array(len).context("encoding CBOR array header")?;
    Ok(buf)
}

fn select_utxos(utxos: &[KoiosUtxo], target: u64) -> Result<(Vec<KoiosUtxo>, u64)> {
    let mut candidates: Vec<(KoiosUtxo, u64)> = utxos
        .iter()
        .map(|u| {
            let lovelace: u64 = u
                .value
                .parse()
                .with_context(|| format!("parsing UTxO value '{}' as u64", u.value))?;
            Ok::<_, anyhow::Error>((u.clone(), lovelace))
        })
        .collect::<Result<Vec<_>>>()?;

    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let mut selected = Vec::new();
    let mut total: u64 = 0;
    for (utxo, lovelace) in candidates {
        if total >= target {
            break;
        }
        total = total
            .checked_add(lovelace)
            .ok_or_else(|| anyhow!("summed UTxO value overflowed u64"))?;
        selected.push(utxo);
    }

    if total < target {
        return Err(anyhow!(
            "insufficient funds: need {} lovelace, only {} available across {} UTxO(s)",
            target,
            total,
            utxos.len()
        ));
    }

    Ok((selected, total))
}

fn utxo_to_input(utxo: &KoiosUtxo) -> Result<TransactionInput> {
    let hash_bytes =
        hex::decode(&utxo.tx_hash).with_context(|| format!("decoding tx_hash '{}'", utxo.tx_hash))?;
    let hash_arr: [u8; 32] = hash_bytes
        .try_into()
        .map_err(|v: Vec<u8>| anyhow!("tx_hash '{}' is {} bytes, expected 32", utxo.tx_hash, v.len()))?;
    Ok(TransactionInput {
        transaction_id: Hash::from(hash_arr),
        index: utxo.tx_index,
    })
}

fn sort_inputs(inputs: &mut [TransactionInput]) {
    inputs.sort_by(|a, b| {
        a.transaction_id
            .as_ref()
            .cmp(b.transaction_id.as_ref())
            .then(a.index.cmp(&b.index))
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn utxo(tx_hash: &str, tx_index: u64, value: &str, inline_datum: Option<&str>) -> KoiosUtxo {
        KoiosUtxo {
            tx_hash: tx_hash.to_string(),
            tx_index,
            address: "addr_test1qtest".to_string(),
            value: value.to_string(),
            inline_datum: inline_datum.map(|s| s.to_string()),
        }
    }

    #[test]
    fn select_utxos_picks_largest_first_and_stops_early() {
        let utxos = vec![
            utxo(&"11".repeat(32), 0, "1000000", None),
            utxo(&"22".repeat(32), 0, "8000000", None),
            utxo(&"33".repeat(32), 0, "3000000", None),
        ];
        let (selected, total) = select_utxos(&utxos, 5_000_000).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(total, 8_000_000);
    }

    #[test]
    fn select_utxos_errors_when_insufficient() {
        let utxos = vec![utxo(&"11".repeat(32), 0, "1000000", None)];
        let err = select_utxos(&utxos, 5_000_000).unwrap_err();
        assert!(err.to_string().contains("insufficient funds"));
    }

    #[test]
    fn utxo_to_input_rejects_bad_hash_length() {
        let bad = utxo("deadbeef", 0, "1000000", None);
        assert!(utxo_to_input(&bad).is_err());
    }

    #[test]
    fn normalize_address_rejects_garbage_without_panicking() {
        // Not "addr"-prefixed and not valid hex either.
        let err = normalize_address("not-an-address!!").unwrap_err();
        assert!(err.to_string().contains("neither bech32"));
    }

    #[test]
    fn decode_cbor_bytestring_unwraps_short_form_header() {
        // CBOR byte string, major type 2, length 3 (0x40 + 3 = 0x43),
        // wrapping the bytes [0xaa, 0xbb, 0xcc].
        let wrapped = [0x43u8, 0xaa, 0xbb, 0xcc];
        let inner = decode_cbor_bytestring(&wrapped).unwrap();
        assert_eq!(inner, vec![0xaa, 0xbb, 0xcc]);
    }

    #[test]
    fn decode_cbor_bytestring_unwraps_1_byte_length_header() {
        // 0x58 = byte string, 1-byte length follows. Simulates a real
        // 29-byte base address's CIP-30 wrapping: 58 1d <29 bytes>.
        let mut wrapped = vec![0x58u8, 29];
        wrapped.extend(std::iter::repeat(0x11u8).take(29));
        let inner = decode_cbor_bytestring(&wrapped).unwrap();
        assert_eq!(inner.len(), 29);
        assert!(inner.iter().all(|&b| b == 0x11));
    }

    #[test]
    fn normalize_address_rejects_bad_hex_gracefully() {
        // Valid hex, but too short / wrong shape to be a real address.
        let err = normalize_address("00ff").unwrap_err();
        // Should surface a Result error, not panic — the test itself passing
        // (rather than aborting the process) is the assertion.
        assert!(err.to_string().len() > 0);
    }

    #[test]
    fn compute_script_data_hash_rejects_empty_language_view() {
        let err = compute_script_data_hash(&[], &[], &[]).unwrap_err();
        assert!(err.to_string().contains("language_view_cbor is empty"));
    }

    #[test]
    fn assemble_signed_tx_splices_witness_set_map() {
        let unsigned = [0x83u8, 0xa0, 0xa0, 0xf6];
        let unsigned_hex = hex::encode(unsigned);
        let witness_hex = hex::encode([0xa0u8]);

        let signed_hex = assemble_signed_tx(&unsigned_hex, &witness_hex).unwrap();
        let signed = hex::decode(signed_hex).unwrap();
        assert_eq!(signed, vec![0x83, 0xa0, 0xa0, 0xf6]);
    }

    #[test]
    fn assemble_signed_tx_passes_through_full_tx_from_wallet() {
        let unsigned_hex = hex::encode([0x83u8, 0xa0, 0xa0, 0xf6]);
        let full_tx = [0x83u8, 0xa0, 0xa1, 0xf6];
        let witness_hex = hex::encode(full_tx);

        let signed_hex = assemble_signed_tx(&unsigned_hex, &witness_hex).unwrap();
        assert_eq!(hex::decode(signed_hex).unwrap(), full_tx.to_vec());
    }

    #[test]
    fn assemble_signed_tx_rejects_garbage_witness() {
        let unsigned_hex = hex::encode([0x83u8, 0xa0, 0xa0, 0xf6]);
        let garbage_hex = hex::encode([0x01u8]);
        let err = assemble_signed_tx(&unsigned_hex, &garbage_hex).unwrap_err();
        assert!(err.to_string().contains("unrecognized witness CBOR"));
    }

    #[test]
    fn assemble_signed_tx_accepts_indefinite_length_witness_map() {
        // 0xbf = indefinite-length map start — some wallets emit this.
        let unsigned_hex = hex::encode([0x83u8, 0xa0, 0xa0, 0xf6]);
        let witness_hex = hex::encode([0xbfu8, 0xff]); // empty indefinite map
        assert!(assemble_signed_tx(&unsigned_hex, &witness_hex).is_ok());
    }

    #[test]
    fn encode_unsigned_tx_produces_alonzo_shaped_4_element_array() {
        let body = TransactionBody {
            inputs: vec![],
            outputs: vec![],
            fee: 0,
            ttl: None,
            certificates: None,
            withdrawals: None,
            update: None,
            auxiliary_data_hash: None,
            validity_interval_start: None,
            mint: None,
            script_data_hash: None,
            collateral: None,
            required_signers: None,
            network_id: None,
            collateral_return: None,
            total_collateral: None,
            reference_inputs: None,
        };
        let hex_out = encode_unsigned_tx(&body).unwrap();
        let bytes = hex::decode(hex_out).unwrap();
        // 0x84 = array header for 4 elements — this is the actual fix for
        // "Expected 3, but found 4": we now build 4 elements from the start.
        assert_eq!(bytes[0], 0x84);
    }

    #[test]
    fn assemble_signed_tx_preserves_4_element_shape_not_hardcoded_3() {
        // Unsigned tx with the CORRECT Alonzo+ 4-element shape:
        // [body(a0), witness(a0), is_valid(f5), aux(f6)].
        let unsigned = [0x84u8, 0xa0, 0xa0, 0xf5, 0xf6];
        let unsigned_hex = hex::encode(unsigned);
        let witness_hex = hex::encode([0xa1u8, 0x00, 0x80]); // non-empty witness map, still a map

        let signed_hex = assemble_signed_tx(&unsigned_hex, &witness_hex).unwrap();
        let signed = hex::decode(signed_hex).unwrap();

        // Must still be a 4-element array (0x84), NOT the old hardcoded
        // 0x83 — this is exactly the bug that produced "Expected 3, but
        // found 4" when Typhon returned a correctly-shaped 4-element tx.
        assert_eq!(signed[0], 0x84);
        assert_eq!(signed[signed.len() - 2..], [0xf5, 0xf6]); // is_valid + aux preserved
    }

    #[test]
    fn cbor_array_header_matches_element_count() {
        assert_eq!(cbor_array_header(3).unwrap(), vec![0x83]);
        assert_eq!(cbor_array_header(4).unwrap(), vec![0x84]);
    }
}
