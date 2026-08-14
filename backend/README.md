# Third Man Protocol — backend (Rust)

An Axum gateway that wraps a Cardano escrow: wallet-based registration → DID → KYC profile
(supplier/buyer) → contract-like agreement → expiring OTP invites → negotiation →
**CIP-8 pre-contract signing by both wallets** → Plutus escrow init + collateral →
release/slash → receipts + governance points → immutable ledger mirror (push/pull).

```bash
cd backend
cp .env.example .env
cargo run
# listening on http://127.0.0.1:8080
curl http://127.0.0.1:8080/health
```

SQLite is created automatically (`thirdman.db`); migrations run on startup.

## Where signatures are required (from CIP-8 / CIP-30)

| Step | What is signed | Cardano API |
| ---- | -------------- | ----------- |
| Register / login | server-issued **nonce** (proves wallet ownership) | `api.signData(addr, nonceHex)` — CIP-8 COSE_Sign1, EdDSA/Ed25519 |
| Pre-contract commitment | **canonical agreement payload** (terms + participants) | `api.signData(addr, payloadHex)` — CIP-8, **both** wallets before escrow |
| Lock funds + collateral | **unsigned transaction body** | `api.signTx(tx, partialSign=true)` — CIP-30 |
| Release / slash | **transaction body** co-signed | `api.signTx(...)` by both parties (release) or arbiter key (slash) |
| Arbiter verdict | **verdict payload** | `api.signData(...)` — CIP-8, consumed by the validator |
| Receipt anchor | **metadata tx** (CIP-10 label) | `api.signTx(...)` so the receipt is pullable on-chain |

CIP-8 verification in `src/crypto.rs` reconstructs the exact `Sig_structure =
["Signature1", body_protected, h'', payload]` (RFC 8152) and verifies the Ed25519
signature with the public key taken from the `COSE_Key`.

## End-to-end flow

1. `POST /auth/challenge` → nonce.
2. Wallet `signData(addr, nonceHex)` → `POST /auth/verify {cose_sign1, cose_key}` →
   mints `did:cardano:<addr>` + session token (`Authorization: Bearer <token>`).
3. `POST /kyc` (supplier/buyer) → `POST /kyc/verify` (operator) → user role set.
4. `POST /agreements` (author tailors terms + `weight` 1..10 + value + max_participants).
   Collateral = `base + bps(value)*weight`, capped.
5. `POST /otp` → expiring shareable code (`max_uses`, TTL). Counterparty `POST /otp/redeem?code=`.
6. Negotiate: `PATCH /agreements/:id/terms` (saves a revision), `GET /agreements/:id/signable`
   → `POST /agreements/:id/sign` (CIP-8) by **both** → `POST /agreements/:id/accept-terms`
   (auto-advances to `agreed` once all signed).
7. Both `POST /collateral/lock`, then `POST /escrow/init` (requires 2 verified signatures
   + 2 locked collateral) → Plutus escrow `locked`.
8. `POST /escrow/:id/complete` → `POST /escrow/:id/release` with both wallets' CIP-30
   partial witnesses → funds released, collateral returned, points awarded, receipt saved,
   release anchored on the ledger mirror.
9. Dispute: `POST /disputes` → arbiter auto-assigned (top trust points) →
   `POST /disputes/:id/oracle` (optional, non-fatal) → `POST /disputes/:id/verdict`
   (arbiter CIP-8-signed) → slash the at-fault party's collateral to the counterparty.
10. `GET /ledger` / `GET /ledger/:tx_hash` to pull immutable records; `POST /ledger/:tx_hash/confirm`
    to mark them on-chain (with block + anchor tx hash).

## Modules

- `crypto.rs` — CIP-8 COSE_Sign1 parse + Ed25519 verify; `did:cardano:` generation.
- `modules/auth.rs` — challenge / verify / sessions.
- `modules/kyc.rs` — supplier/buyer profiles + verification.
- `modules/agreements.rs` — tailor + revise terms, weight-scaled collateral math.
- `modules/otp.rs` — expiring invites with capacity.
- `modules/negotiation.rs` — accept-terms → `agreed` state machine.
- `modules/signing.rs` — CIP-8 agreement signing by both wallets.
- `modules/collateral.rs` — lock / return / slash.
- `modules/escrow.rs` — Plutus escrow init + lock/release/slash; `TxBuilder` trait.
- `modules/dispute.rs` — raise + arbiter pool + oracle + CIP-8 verdict.
- `modules/points.rs` — governance trust points.
- `modules/receipts.rs` — wallet receipts + CIP-10 anchor.
- `modules/ledger.rs` — append-only push/pull store.

## What is a real implementation vs a stub

- **Real:** CIP-8 signature verification, DID, KYC, agreements, OTP, negotiation,
  collateral math, dispute/arbiter/verdict, points, receipts, immutable ledger mirror.
- **Stubbed behind a trait:** the actual Cardano transaction CBOR in `escrow.rs`
  (`StubTxBuilder`). It returns structured JSON describing the intended tx so the
  whole flow is exercisable locally. Swap in a real builder (`pallas` or
  `cardano-serialization-lib`) and a node/blockfrost submitter before testnet.
- **Oracle** returns a deterministic mock for `source:"mock"`; wire `ORACLE_ENDPOINT`
  (add `reqwest`) for real pulls. Failures are non-fatal by design.

## Suggestions to make it better

1. **Real on-chain builder.** Replace `StubTxBuilder` with a `pallas`-based builder that
   emits real CBOR for lock/release/slash against the Aiken validator, and submit via
   Blockfrost. The datum should commit the `terms_hash` + participant payment keys + the
   arbiter public key so the validator can enforce `verifyEd25519` on the verdict.
2. **Hash-address binding.** Tighten `crypto::address_matches` to actually decode bech32
   and compare the address payload bytes from the CIP-8 protected header (drop the
   `addr*` shortcut) so a signature is provably bound to the registered address.
3. **Nonce replay & rate limiting.** Nonces are single-use + 5-min TTL, but add per-IP
   rate limiting and signed challenge JWTs to harden the auth path.
4. **Collateral on-chain.** Currently collateral is recorded off-chain; lock it for real
   as a second UTXO at the validator (or a separate collateral script) so slashing is
   enforced by Plutus, not just mirrored.
5. **Arbiter stake & slashing of arbiters.** Require arbiters to stake governance points
   to enter the pool; a wrong verdict (overturned on appeal) should slash their stake.
6. **Multi-party agreements.** `max_participants` already supports >2; generalize the
   release witness requirement to N-of-M via the validator's redeemer.
7. **ZK / Merkle receipts.** Instead of storing full receipt JSON, anchor a Merkle root of
   many receipts per tx to cut on-chain cost; serve proofs from the mirror.
8. **Idempotency keys** on all mutating endpoints to make the gateway safe to retry.
9. **Postgres** for production (SQLx is already used; switch the URL + features).
10. **Observability:** OpenTelemetry tracing, plus a `/metrics` endpoint.

## Verify it runs

```bash
curl http://127.0.0.1:8080/health
curl -X POST http://127.0.0.1:8080/auth/challenge \
  -H 'content-type: application/json' \
  -d '{"address":"addr_test1q...","purpose":"register"}'
```
