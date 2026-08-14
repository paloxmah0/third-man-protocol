# Third Man Protocol — Smart Contract Specification

This document describes exactly what the on-chain validator must do, the datum/redeemer shapes, and how the off-chain code interacts with it. Use this as your blueprint to write the validator manually (in Aiken, Plutus, OpShin, or raw Plutus Core).

## 1. Validator Overview

The escrow validator is a **Plutus V3 spending validator**. It sits at a script address on Cardano. ADA is locked there with a datum. To spend the locked ADA, the spender provides a redeemer that the validator checks.

### What the validator enforces

| Action | Who | Conditions |
|--------|-----|------------|
| **Deposit** (lock tx) | Anyone | Pays ADA to the script address with the DealDatum as inline datum. No validator execution — just creating a UTxO. |
| **Release** | Both parties | Both parties' signatures present in the tx. Status = Active. Pays to the recipient. |
| **Slash** | Arbiter | Arbiter's Ed25519 signature over the verdict. Status = Disputed. Pays to the beneficiary. |
| **Refund** | Anyone | Current time > funding_deadline. Status = PendingFunding. Pays back to depositor. |
| **Dispute** | Any party | Party's signature present. Status = Active. Re-locks with status = Disputed. |

## 2. DealDatum (the on-chain datum)

This is attached as an **inline datum (CIP-32)** to the escrow UTxO when the lock tx is submitted. The full datum is retrievable directly from the UTxO — no separate datum witness needed.

### Aiken struct

```aiken
type Party {
  Party {
    address: ByteArray,        // bech32 payment address (hex-encoded bytes)
    role: ByteArray,           // "buyer" or "supplier" (hex-encoded bytes)
    collateral_amount: Int,    // lovelace
  }
}

type DealDatum {
  DealDatum {
    deal_id: ByteArray,              // unique deal identifier (hex bytes)
    parties: List<Party>,            // all parties with their addresses + collateral
    total_value: Int,                // total lovelace locked in escrow
    release_condition: Int,          // 0=mutual_confirm, 1=oracle, 2=timeout_to_dispute, 3=hybrid_arbiter
    document_hash: ByteArray,        // blake2b-256 of the agreement terms (terms_hash)
    attachment_hashes: List<ByteArray>,  // SHA-256 hashes of all attachments
    dispute_window: Int,             // dispute window in days
    funding_deadline: Int,           // POSIX timestamp (seconds) — funding must complete before this
    funded_so_far: Int,              // lovelace deposited so far (for multi-depositor)
    status: Int,                     // see status enum below
    created_at: Int,                 // POSIX timestamp (seconds)
  }
}
```

### Status enum (as integers, not Aiken enums — to avoid the compiler bug)

| Value | Name | Description |
|-------|------|-------------|
| 0 | PendingFunding | Funds not yet fully deposited |
| 1 | Active | Fully funded, deal is live |
| 2 | Releasing | Release tx in progress |
| 3 | Completed | Funds released, deal done |
| 4 | Disputed | Dispute raised, awaiting arbiter |
| 5 | Slashed | Arbiter verdict executed, collateral slashed |
| 6 | Expired | Funding deadline passed, refunded |

### Release condition enum

| Value | Name |
|-------|------|
| 0 | mutual_confirm |
| 1 | oracle |
| 2 | timeout_to_dispute |
| 3 | hybrid_arbiter |

### TypeScript equivalent (for the off-chain side)

This is in `web/src/lib/datum.ts` and MUST match the Aiken struct field-for-field:

```typescript
const DealDatumSchema = Data.Object({
  deal_id: Data.Bytes(),
  parties: Data.Array(Data.Object({
    address: Data.Bytes(),
    role: Data.Bytes(),
    collateral_amount: Data.Integer(),
  })),
  total_value: Data.Integer(),
  release_condition: Data.Integer(),
  document_hash: Data.Bytes(),
  attachment_hashes: Data.Array(Data.Bytes()),
  dispute_window: Data.Integer(),
  funding_deadline: Data.Integer(),
  funded_so_far: Data.Integer(),
  status: Data.Integer(),
  created_at: Data.Integer(),
});
```

### Plutus Data encoding

Aiken structs encode as `Constr(0, [field1, field2, ...])` in Plutus Data. The field order MUST match. Lucid's `Data.to()` handles this automatically if the TypeScript schema matches.

Example CBOR for a DealDatum:
```
Constr(0, [
  deal_id: bytes,
  parties: List [
    Constr(0, [address_bytes, role_bytes, collateral_int]),
    ...
  ],
  total_value: int,
  release_condition: int,
  document_hash: bytes,
  attachment_hashes: List [hash1, hash2, ...],
  dispute_window: int,
  funding_deadline: int,
  funded_so_far: int,
  status: int,
  created_at: int,
])
```

## 3. Redeemer (Action)

The redeemer is provided when spending the escrow UTxO. It tells the validator which action is being performed.

### Aiken enum

```aiken
type Action {
  Deposit { amount: Int }
  Release { recipient: ByteArray }
  Slash {
    at_fault: ByteArray,
    beneficiary: ByteArray,
    arbiter_pubkey: ByteArray,
    arbiter_signature: ByteArray,
    verdict_hash: ByteArray,
  }
  Refund
  Dispute { raised_by: ByteArray }
}
```

### Plutus Data encoding (Constr indices)

| Action | Constr index | Fields |
|--------|-------------|--------|
| Deposit | 0 | `[amount: Int]` |
| Release | 1 | `[recipient: ByteArray]` |
| Slash | 2 | `[at_fault, beneficiary, arbiter_pubkey, arbiter_signature, verdict_hash]` |
| Refund | 3 | `[]` (no fields) |
| Dispute | 4 | `[raised_by: ByteArray]` |

### TypeScript equivalent (for the off-chain side)

```typescript
// In web/src/lib/datum.ts — uses Constr directly

// Release = Constr(1, [recipient_bytes])
Data.to(new Constr(1, [recipientAddress]))

// Deposit = Constr(0, [amount])
Data.to(new Constr(0, [BigInt(amount)]))

// Slash = Constr(2, [at_fault, beneficiary, arbiter_pubkey, arbiter_signature, verdict_hash])
Data.to(new Constr(2, [atFault, beneficiary, arbiterPubkey, arbiterSignature, verdictHash]))

// Refund = Constr(3, [])
Data.to(new Constr(3, []))

// Dispute = Constr(4, [raised_by])
Data.to(new Constr(4, [raisedBy]))
```

## 4. Validator Logic (pseudocode)

```
spend(datum: DealDatum, redeemer: Action, ctx: ScriptContext):

  let signatories = ctx.transaction.extra_signatories  // addresses that signed the tx
  let now = ctx.transaction.time_range                  // current time

  switch redeemer:

    case Deposit { amount }:
      // Anyone can deposit. This is actually the LOCK TX — it creates the UTxO.
      // The validator doesn't execute on deposit (no prior UTxO to spend).
      // This redeemer is used when topping up (multi-depositor).
      assert(datum.funded_so_far + amount <= datum.total_value)
      // Output must contain the updated datum with incremented funded_so_far
      // If funded_so_far + amount == total_value → status = Active (1)

    case Release { recipient }:
      // Mutual confirm — both parties must sign
      assert(datum.status == 1)  // Active
      assert(recipient is in datum.parties)
      assert(all parties in datum.parties have their address in signatories)
      // Output pays the locked funds to the recipient

    case Slash { at_fault, beneficiary, arbiter_pubkey, arbiter_signature, verdict_hash }:
      // Arbiter verdict — slashes at_fault's collateral to beneficiary
      assert(datum.status == 4)  // Disputed
      assert(beneficiary is in datum.parties)
      assert(at_fault is in datum.parties)
      // Verify the arbiter's Ed25519 signature over the verdict_hash
      assert(ed25519_verify(arbiter_pubkey, arbiter_signature, verdict_hash))
      // Output pays the locked funds to the beneficiary

    case Refund:
      // Funding deadline passed without full funding
      assert(datum.status == 0)  // PendingFunding
      assert(now > datum.funding_deadline)
      // Output pays back to the depositor(s)

    case Dispute { raised_by }:
      // Any party raises a dispute
      assert(datum.status == 1)  // Active
      assert(raised_by is in datum.parties)
      assert(raised_by in signatories)  // the raiser must sign
      // Output re-locks the funds with updated datum (status = Disputed (4))
```

## 5. Lock Transaction (Deposit to Script)

This is the transaction that locks ADA into the escrow. It does NOT spend a script UTxO — it creates one.

### Inputs
- Depositor's payment UTxO(s) — from `wallet.getUtxos()`

### Outputs
- **Escrow output**: pays `total_value` lovelace to the **script address**, with the **DealDatum as inline datum** (CIP-32)
- **Change output**: remaining ADA back to the depositor

### What the off-chain code does (Lucid)

```typescript
const tx = await lucid.newTx()
  .pay.ToContract(
    scriptAddress,                              // computed from validatorToAddress("Preprod", script)
    { kind: "inline", value: datumCborHex },    // CIP-32 inline datum
    { lovelace: totalValue.toString() }         // ADA amount
  )
  .complete();                                  // auto-selects UTxOs from wallet

// Sign + submit
const signedTx = await tx.sign.withWallet().complete();
const txHash = await signedTx.submit();
```

### What gets stored on-chain
- A UTxO at the script address containing:
  - `value`: the locked ADA
  - `inline datum`: the full DealDatum (retrievable without a separate datum witness)

## 6. Release Transaction (Spend from Script)

This is the transaction that releases funds from the escrow. It DOES spend the script UTxO.

### Inputs
- The escrow UTxO (script address, with inline datum)
- Both parties' signatures (as required signers in the tx body)

### Outputs
- Payment to the recipient (one of the parties)

### Redeemer
- `Release { recipient }` = `Constr(1, [recipient_bytes])`

### What the off-chain code does (Lucid)

```typescript
const escrowUtxo = await findEscrowUtxo(lucid);  // find the UTxO at script address

const tx = await lucid.newTx()
  .collectFrom([escrowUtxo], releaseRedeemer)    // spend the script UTxO with the redeemer
  .pay.ToAddress(recipientAddress, { lovelace: lockedAmount })
  .attach.SpendingValidator(escrowScript)         // attach the compiled validator
  .complete();

// Both parties must sign
const signedTx = await tx.sign.withWallet().complete();
const txHash = await signedTx.submit();
```

### What the validator checks (on-chain)
1. `datum.status == 1` (Active)
2. `recipient` is a party in `datum.parties`
3. All parties' addresses are in `ctx.transaction.extra_signatories`
4. If all pass → tx succeeds, funds pay out

## 7. Slash Transaction (Arbiter Verdict)

Same structure as Release, but with a Slash redeemer and arbiter signature verification.

### Redeemer
- `Slash { at_fault, beneficiary, arbiter_pubkey, arbiter_signature, verdict_hash }`
- = `Constr(2, [at_fault, beneficiary, arbiter_pubkey, arbiter_signature, verdict_hash])`

### What the validator checks
1. `datum.status == 4` (Disputed)
2. `beneficiary` is a party
3. `at_fault` is a party
4. `ed25519_verify(arbiter_pubkey, arbiter_signature, verdict_hash)` — the arbiter's CIP-8 signature over the verdict is valid
5. If all pass → funds pay to beneficiary, at_fault's collateral is slashed

## 8. Dispute Transaction

Spends the escrow UTxO and re-locks it with an updated datum (status = Disputed). Funds don't move — they stay at the script address.

### Redeemer
- `Dispute { raised_by }` = `Constr(4, [raised_by_bytes])`

### What the validator checks
1. `datum.status == 1` (Active)
2. `raised_by` is a party
3. `raised_by` is in `signatories`
4. If all pass → output re-locks funds at script address with `status = 4` (Disputed)

## 9. Refund Transaction

Returns funds to depositors if the funding deadline passes.

### Redeemer
- `Refund` = `Constr(3, [])`

### What the validator checks
1. `datum.status == 0` (PendingFunding)
2. Current time > `funding_deadline`
3. If all pass → funds return to depositor

## 10. How to Write the Validator

### Option A: Aiken (if the compiler bug is fixed)

```aiken
// File: validators/escrow.ak

use cardano/transaction.{Transaction, OutputReference}

type Party {
  Party { address: ByteArray, role: ByteArray, collateral_amount: Int }
}

type DealDatum {
  DealDatum {
    deal_id: ByteArray,
    parties: List<Party>,
    total_value: Int,
    release_condition: Int,
    document_hash: ByteArray,
    attachment_hashes: List<ByteArray>,
    dispute_window: Int,
    funding_deadline: Int,
    funded_so_far: Int,
    status: Int,
    created_at: Int,
  }
}

type Action {
  Deposit { amount: Int }
  Release { recipient: ByteArray }
  Slash {
    at_fault: ByteArray,
    beneficiary: ByteArray,
    arbiter_pubkey: ByteArray,
    arbiter_signature: ByteArray,
    verdict_hash: ByteArray,
  }
  Refund
  Dispute { raised_by: ByteArray }
}

validator escrow {
  spend(datum: Option<DealDatum>, redeemer: Action, _utxo: OutputReference, self: Transaction) {
    expect Some(d) = datum

    when redeemer is {
      Deposit { amount } -> {
        if d.funded_so_far + amount > d.total_value { fail }
      }
      Release { recipient } -> {
        if d.status != 1 { fail }
        // Check recipient is a party
        // Check all parties signed
      }
      Slash { at_fault, beneficiary, arbiter_pubkey, arbiter_signature, verdict_hash } -> {
        if d.status != 4 { fail }
        // Verify arbiter signature
      }
      Refund -> {
        if d.status != 0 { fail }
        // Check time > funding_deadline
      }
      Dispute { raised_by } -> {
        if d.status != 1 { fail }
        // Check raised_by is a party + signed
      }
    }
  }
}
```

Compile with: `aiken build` → produces `plutus.json` with the compiled code + hash.

### Option B: Raw Plutus Data (avoids the typed datum bug)

If you use `Data` types instead of custom types, the Aiken compiler works:

```aiken
validator escrow {
  spend(datum_raw: Option<Data>, redeemer_raw: Data, _utxo: OutputReference, self: Transaction) {
    // Manually decode the datum and redeemer from raw Data
    // Use Constr pattern matching or list indexing
    // This avoids the Aiken compiler bug with custom types in signatures
  }
}
```

### Option C: OpShin (Python → Plutus)

```python
from opshin.prelude import *
from opshin.std.builtins import *

def validator(datum: Data, redeemer: Data, ctx: ScriptContext) -> None:
    # Decode datum and redeemer manually
    # Check conditions
    # fail() if invalid
    pass
```

### After Compiling

1. Copy the `plutus.json` to `onchain/plutus.json`
2. Extract the script hash from the blueprint
3. Update `backend/.env`:
   ```
   ESCROW_VALIDATOR_SCRIPT_HASH=<real_hash_from_blueprint>
   ```
4. The frontend automatically loads the blueprint from `onchain/plutus.json` via `web/src/lib/script.ts`
5. Lucid computes the script address via `validatorToAddress("Preprod", script)`

## 11. Key Rules

1. **Field order matters** — the DealDatum fields in Aiken must match the TypeScript `Data.Object` exactly, in the same order
2. **Constr indices matter** — Deposit=0, Release=1, Slash=2, Refund=3, Dispute=4
3. **Inline datum (CIP-32)** — the datum is attached directly to the UTxO, not as a separate hash
4. **Ed25519 verification** — for Slash, the arbiter's signature is verified on-chain using `ed25519_verify`
5. **Extra signatories** — `ctx.transaction.extra_signatories` contains the addresses that must sign the tx
6. **Time check** — for Refund, compare `ctx.transaction.time_range` against `funding_deadline`
7. **Script hash is deterministic** — changing any validator logic changes the hash + address. Already-locked UTxOs under the old hash become unreachable.
