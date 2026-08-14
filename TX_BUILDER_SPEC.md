# Pallas Transaction Builder — Specification for Manual Implementation

This document describes exactly what the `tx_builder.rs` module needs to do, the inputs/outputs, and how the frontend calls it. Write this in Rust using Pallas 0.30 + reqwest for Koios API calls.

## Architecture

```
Frontend (browser)          Backend (Rust + Pallas)           Cardano Preprod
─────────────────          ──────────────────────             ───────────────
                           
1. POST /escrow/init  ──→  Creates DealDatum in DB
                           (JSON, stored in smart_contracts.deal_datum_json)
                           
2. GET /escrow/:id/   ──→  Reads DealDatum from DB
   lock-tx                 Converts to Plutus Data CBOR
                           Fetches depositor's UTxOs from Koios
                           Builds unsigned Babbage tx with Pallas:
                             - inputs: selected UTxOs
                             - output 1: script_addr + lovelace + inline datum
                             - output 2: change_addr + remaining lovelace
                             - fee: estimated
                           Returns: { tx_cbor: "hex...", contribution_id, deal_datum, ... }
                           
3. wallet.api.signTx  ←──  Frontend receives tx_cbor
   (txCbor, true)          Wallet popup appears
                           User approves
                           Returns: witness_set_cbor (hex)
                           
4. POST /escrow/:id/  ──→  Receives { contribution_id, witness }
   submit-lock-tx          Assembles: unsigned_tx_body + witness → signed_tx
                           Submits signed_tx to Koios: POST /submittx
                           Returns: { tx_hash: "real_hash", fully_funded: true/false }
```

## File: `backend/src/modules/tx_builder.rs`

### Dependencies (already in Cargo.toml)

```toml
pallas = "0.30"
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
hex = "0.4"
```

### Key Pallas types (Pallas 0.30)

```rust
use pallas::ledger::addresses::Address;
use pallas::crypto::hash::Hash;  // Hash<32> for tx hashes
use pallas::codec::minicbor;     // Encoder + Decoder
use pallas::ledger::primitives::babbage::{
    TransactionBody,           // The tx body struct
    TransactionInput,           // { transaction_id: Hash<32>, index: u64 }
    TransactionOutput,          // Type alias = PseudoTransactionOutput<...>
    PostAlonzoTransactionOutput,// The actual output struct
    DatumOption,                // Hash(Hash<32>) or Data(CborWrap<PlutusData>)
    NetworkId,                  // One (mainnet) or Two (testnet)
    Value,                      // Coin(u64) or Multiasset(u64, ...)
};
use pallas::ledger::primitives::alonzo::PlutusData;
```

### TransactionBody fields (from Pallas 0.30 docs)

```rust
TransactionBody {
    inputs: Vec<TransactionInput>,           // NOT MaybeIndefArray — just Vec
    outputs: Vec<TransactionOutput>,         // Vec of PseudoTransactionOutput
    fee: u64,                                // NOT BigInt — just u64
    ttl: Option<u64>,
    certificates: Option<Vec<Certificate>>,
    withdrawals: Option<KeyValuePairs<Bytes, u64>>,
    update: Option<Update>,
    auxiliary_data_hash: Option<Bytes>,
    validity_interval_start: Option<u64>,
    mint: Option<KeyValuePairs<Hash<28>, KeyValuePairs<Bytes, i64>>>,
    script_data_hash: Option<Hash<32>>,
    collateral: Option<Vec<TransactionInput>>,
    required_signers: Option<Vec<Hash<28>>>,
    network_id: Option<NetworkId>,           // NetworkId::Two for testnet
    collateral_return: Option<TransactionOutput>,
    total_collateral: Option<u64>,
    reference_inputs: Option<Vec<TransactionInput>>,
}
```

### TransactionOutput (type alias)

```rust
// TransactionOutput = PseudoTransactionOutput<PseudoPostAlonzoTransactionOutput<Value, DatumOption<PlutusData>, PseudoScript<NativeScript>>>
// It's an enum with two variants:
//   Legacy(LegacyTransactionOutput)
//   PostAlonzo(PostAlonzoTransactionOutput)
//
// Use PostAlonzo for Babbage era:

TransactionOutput::PostAlonzo(PostAlonzoTransactionOutput {
    address: Bytes,          // Address.to_vec().into() → Bytes
    value: Value::Coin(u64), // or Value::Multiasset(u64, ...)
    datum_option: Option<DatumOption>,
    script_ref: Option<...>, // None for our case
})
```

### DatumOption (inline datum = CIP-32)

```rust
// For inline datum:
DatumOption::Data(CborWrap(plutus_data))
// where plutus_data is a PlutusData value

// For datum hash (not what we want):
DatumOption::Hash(Hash<32>)
```

### Building PlutusData from the DealDatum

The DealDatum is stored as JSON in the DB. You need to convert it to PlutusData CBOR.

The Aiken struct encodes as Constr(0, [fields...]) in Plutus Data. The field order must match:

```
DealDatum = Constr(0, [
    deal_id: Bytes,                    // ByteArray → bytes
    parties: List[                     // List of Party
        Constr(0, [                    // Party = Constr(0, [address, label])
            address: Bytes,
            label: Bytes,
        ]),
    ],
    total_value: Int,                  // u64 → Int
    release_units: List[               // List of ReleaseUnit
        Constr(0, [                    // ReleaseUnit = Constr(0, [...])
            unit_id: Bytes,
            allocation: Constr(0, [    // Allocation = Constr(0, [recipient, amount])
                recipient: Bytes,
                amount: Int,
            ]),
            condition: ...,            // Enum — see below
            proof: Constr(0, [         // ProofRequirement = Constr(0, [...])
                required: Bool,
                attachment_hash: Bytes,
                submitted_by: Bytes,
                rejection_count: Int,
                max_attempts: Int,
                accepted: Bool,
            ]),
            claimed: Bool,
        ]),
    ],
    release_condition: ...,            // Enum — see below
    document_hash: Bytes,
    attachment_hashes: List[Bytes],
    dispute_window: Int,
    funding_deadline: Int,
    funded_so_far: Int,
    status: Int,
    created_at: Int,
])
```

### Enum encoding (Aiken enums → Plutus Constr)

Aiken enums encode as Constr(index, [fields]) where index = constructor order:

```
// UnitCondition:
NoCondition       = Constr(0, [])
ApprovalRequired  = Constr(1, [])
ProofRequired     = Constr(2, [])
TimeGated         = Constr(3, [unlock_at: Int])
CycleGated        = Constr(4, [due_at: Int])

// ReleaseCondition:
MutualConfirm           = Constr(0, [])
OracleConfirm           = Constr(1, [oracle_pubkey: Bytes])
TimeoutDispute          = Constr(2, [timeout: Int])
HybridArbiter           = Constr(3, [arbiter_pubkey: Bytes, fee_bps: Int])
TimeVesting             = Constr(4, [])
RecurringSubscription   = Constr(5, [period: Int])

// Action (redeemer):
Deposit         = Constr(0, [amount: Int])
ClaimUnit       = Constr(1, [unit_id: Bytes, recipient: Bytes])
SubmitProof     = Constr(2, [unit_id: Bytes, attachment_hash: Bytes, submitted_by: Bytes])
ReviewProof     = Constr(3, [unit_id: Bytes, accepted: Bool, reason_hash: Bytes])
                // NOTE: Bool in Plutus = Constr(0,[]) for True, Constr(1,[]) for False
RaiseDispute    = Constr(4, [raised_by: Bytes])
ArbiterResolve  = Constr(5, [unit_id, recipient, arbiter_pubkey, arbiter_signature, verdict_hash])
Refund          = Constr(6, [])
```

**IMPORTANT:** In Plutus Data, `Bool` is NOT a primitive. It's:
- `True` = `Constr(0, [])`  
- `False` = `Constr(1, [])`

### Function 1: `build_lock_tx` (lock/deposit)

**Input:** depositor's bech32 address, script address, lovelace amount, datum CBOR hex
**Output:** unsigned transaction CBOR hex (string)

```rust
pub async fn build_lock_tx(
    koios: &KoiosProvider,
    depositor_address: &str,    // bech32, e.g. "addr_test1q..."
    script_address: &str,       // "addr_test1wzuwwnmm7msjvp2m4v292pl9nsal376qqkwzywwhwtk0aysufmxqn"
    lovelace_amount: u64,       // e.g. 5000000 for 5 ADA
    datum_cbor_hex: &str,       // hex-encoded Plutus Data CBOR (the DealDatum)
) -> Result<String>            // returns hex-encoded unsigned tx CBOR
```

Steps:
1. Fetch depositor's UTxOs: `koios.get_wallet_utxos(depositor_address)`
2. Select UTxOs that sum to >= lovelace_amount + fee (estimate fee = 200000 lovelace)
3. Parse `Address::from_bech32()` for script + change addresses
4. Decode datum: `minicbor::Decoder::new(&datum_bytes).decode::<PlutusData>()`
5. Build inputs: `Vec<TransactionInput>` from selected UTxOs (tx_hash + tx_index)
6. Build outputs:
   - Output 1: `TransactionOutput::PostAlonzo(PostAlonzoTransactionOutput { address: script_addr, value: Value::Coin(lovelace_amount), datum_option: Some(DatumOption::Data(CborWrap(plutus_data))), script_ref: None })`
   - Output 2 (change): same but `Value::Coin(change_amount)`, `datum_option: None`
7. Build `TransactionBody { inputs, outputs, fee, network_id: Some(NetworkId::Two), ... all else None }`
8. Encode body to CBOR: `encoder.encode(body)?`
9. Wrap as full tx: `[body_bytes, empty_witness_set_bytes, null]`
   - Empty witness set = `0xa0` (CBOR map with 0 entries)
10. Return `hex::encode(tx_cbor)`

### Function 2: `build_spend_tx` (release/dispute/proof)

**Input:** script address, payout info, relock info, change address
**Output:** unsigned transaction CBOR hex

```rust
pub async fn build_spend_tx(
    koios: &KoiosProvider,
    script_address: &str,                  // script bech32 address
    payout_address: Option<&str>,          // who gets paid (for ClaimUnit/ArbiterResolve)
    payout_amount: Option<u64>,            // how much
    relock_datum_cbor_hex: Option<&str>,   // updated datum for re-locking
    relock_amount: Option<u64>,            // how much to re-lock
    change_address: &str,                  // who pays the fee
) -> Result<String>
```

Steps:
1. Fetch script UTxOs: `koios.get_script_utxos(script_address)` — find the one with inline_datum
2. Fetch change address UTxOs (for fee payment)
3. Build inputs: [escrow_utxo, fee_utxo]
4. Build outputs:
   - Payout output (if any): `TransactionOutput::PostAlonzo(...)` with `Value::Coin(payout_amount)`
   - Re-lock output (if any): `TransactionOutput::PostAlonzo(...)` with `DatumOption::Data(CborWrap(updated_plutus_data))`
   - Change output: remaining lovelace
5. Same TransactionBody + CBOR encoding as build_lock_tx

**NOTE:** For spend txs, the validator script must also be attached as a reference script or in the witness set. This is complex — for now, just build the body. The wallet + backend need to attach the script witness separately.

### Function 3: `assemble_signed_tx`

**Input:** unsigned tx CBOR hex, witness CBOR hex (from wallet's signTx)
**Output:** signed tx CBOR hex (ready to submit)

```rust
pub fn assemble_signed_tx(unsigned_tx_cbor: &str, witness_cbor: &str) -> Result<String>
```

Steps:
1. Decode unsigned tx: `[body_bytes, old_witness_bytes, metadata_bytes?]`
2. Replace old witness with new witness from wallet
3. Re-encode: `[body_bytes, witness_bytes, metadata_bytes_or_null]`
4. Return hex

**IMPORTANT:** The wallet's `signTx(txCbor, partialSign=true)` returns a `transaction_witness_set` CBOR hex. This is NOT a full transaction — it's just the witness set. You must combine it with the original body.

However, some wallets (like Typhon) might return the full signed transaction. Check:
- If the witness starts with `82` (CBOR array of 2), it's `[body, witness]` — a full tx
- If it starts with `a0` or `a1` (CBOR map), it's just the witness set — needs assembling

Handle both cases.

### Function 4: Koios provider (`koios.rs`)

Already written in `backend/src/modules/koios.rs`. Key methods:

```rust
pub async fn get_utxos_at(&self, address: &str) -> Result<Vec<KoiosUtxo>>
pub async fn submit_tx(&self, tx_cbor_hex: &str) -> Result<String>
```

**Koios API endpoints (Preprod):**
- `POST https://preprod.koios.rest/api/v1/address_info` — body: `[{"_addresses": ["addr_test1..."]}]` — returns UTxOs
- `POST https://preprod.koios.rest/api/v1/submittx` — body: raw CBOR bytes (not hex) — returns tx hash

**KoiosUtxo fields:**
```rust
pub struct KoiosUtxo {
    pub tx_hash: String,      // hex, e.g. "a1b2c3..."
    pub tx_index: u64,        // output index
    pub address: String,      // bech32
    pub value: String,        // lovelace as string (Koios returns it as string)
    pub inline_datum: Option<String>,  // CBOR hex if present
}
```

### How the frontend calls these

**Lock tx flow:**
```typescript
// 1. Init escrow (creates DealDatum in DB)
const sc = await api.escrow.init(agreementId);

// 2. Build lock tx (backend uses Pallas + Koios)
const lockTx = await api.escrow.buildLockTx(sc.id);
// Returns: { tx_cbor: "hex...", contribution_id: "uuid", deal_datum: {...}, ... }

// 3. Wallet signs
const witness = await wallet.api.signTx(lockTx.unsigned_tx.tx_cbor, true);

// 4. Submit (backend assembles + submits to Koios)
const result = await api.escrow.submitLockTx(sc.id, lockTx.contribution_id, witness);
// Returns: { tx_hash: "real_hash", fully_funded: true }
```

**Spend tx flow (ClaimUnit):**
```typescript
// 1. Build spend tx (backend finds escrow UTxO + builds spend CBOR)
//    This endpoint doesn't exist yet — you need to add it
const spendTx = await fetch(`/escrow/${scId}/build-spend-tx`, {
  method: "POST",
  body: JSON.stringify({
    action: "ClaimUnit",
    unit_id: "unit_0",
    recipient: myAddress,
  }),
}).then(r => r.json());

// 2. Wallet signs
const witness = await wallet.api.signTx(spendTx.tx_cbor, true);

// 3. Submit
const result = await fetch(`/escrow/${scId}/submit-spend-tx`, {
  method: "POST",
  body: JSON.stringify({ witness }),
}).then(r => r.json());
```

### Backend endpoints needed

| Method | Path | What it does |
|--------|------|-------------|
| POST | `/escrow/init` | Creates DealDatum in DB (already exists) |
| GET | `/escrow/:id/lock-tx` | Calls `tx_builder::build_lock_tx()` + returns CBOR (already exists, needs real impl) |
| POST | `/escrow/:id/submit-lock-tx` | Calls `assemble_signed_tx()` + `koios.submit_tx()` (already exists, needs real impl) |
| POST | `/escrow/:id/build-spend-tx` | Calls `tx_builder::build_spend_tx()` + returns CBOR (NEEDS TO BE ADDED) |
| POST | `/escrow/:id/submit-spend-tx` | Calls `assemble_signed_tx()` + `koios.submit_tx()` (NEEDS TO BE ADDED) |

### Script address (confirmed)

```
addr_test1wzuwwnmm7msjvp2m4v292pl9nsal376qqkwzywwhwtk0aysufmxqn
```

Script hash: `b8e74f7bf6e126055bab145507e59c3bf8fb40059c2239d772ecfe92`

### Key gotchas

1. **fee is `u64`** not BigInt — the TransactionBody struct uses `u64` directly
2. **outputs is `Vec<TransactionOutput>`** not `MaybeIndefArray` — Pallas 0.30 changed this
3. **NetworkId::Two** for testnet, **NetworkId::One** for mainnet
4. **Bool in Plutus Data** = `Constr(0, [])` for True, `Constr(1, [])` for False — NOT a primitive
5. **Koios returns `value` as a string** — parse it with `value.parse::<u64>()`
6. **The wallet's signTx returns a witness set, not a full tx** — you must assemble them
7. **Koios submit expects raw bytes** (not hex) — `body(hex::decode(cbor_hex)?)`
8. **Address::from_bech32()** returns the Address struct — use `.to_vec()` to get bytes for the output
9. **Hash<32>** for tx hashes — construct from `[u8; 32]` via `Hash::from(arr)`
10. **No stub fallbacks** — if Pallas fails, return the error. Don't fake it.

### Files to write/modify

1. **`backend/src/modules/tx_builder.rs`** — the main file. Write `build_lock_tx()`, `build_spend_tx()`, `assemble_signed_tx()`. Use Pallas + Koios.
2. **`backend/src/modules/koios.rs`** — already written, may need tweaks to the Koios API calls
3. **`backend/src/modules/escrow.rs`** — update `build_lock_tx()` and `submit_lock_tx()` endpoints to call the real Pallas functions instead of StubTxBuilder. Remove all `StubTxBuilder` code.
4. **Add spend tx endpoints** — `POST /escrow/:id/build-spend-tx` and `POST /escrow/:id/submit-spend-tx`

### What to remove

- `StubTxBuilder` struct and its `TxBuilder` trait impl — delete entirely
- `TxCtx` struct — not needed
- `TxDraft` struct — not needed
- All `builder.build_lock_tx(&ctx)?` calls — replaced by direct `tx_builder::build_lock_tx(...)` calls
- All `builder.submit(...)` calls — replaced by `koios.submit_tx(...)`
