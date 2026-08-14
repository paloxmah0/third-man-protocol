# Third Man Protocol — How to Run & Rules for New Sessions

## Quick Start (from a fresh terminal)

### 1. Start the Backend

```bash
cd D:\third-man-app\backend
cargo run
```
- Runs on `http://127.0.0.1:8080`
- SQLite DB created automatically (`thirdman.db`)
- Migrations run on startup
- Do NOT delete `thirdman.db` between restarts (users + agreements are preserved)
- If you change Rust code: `cargo build` first, then run the `.exe` directly:
  ```bash
  D:\third-man-app\backend\target\debug\third-man-backend.exe
  ```

### 2. Start the Frontend

In a separate terminal:
```bash
cd D:\third-man-app\web
npm run dev
```
- Runs on `http://localhost:5173`
- Vite proxies all `/auth`, `/agreements`, `/escrow`, etc. calls to the backend on 8080
- If `node_modules` is missing: `npm install` first
- If Vite won't start: check that `D:\` drive is connected (it's an external drive)

### 3. Or Start Both at Once

```bash
cd D:\third-man-app
npm run dev
```
- Uses `concurrently` to run both backend + frontend

### 4. Open in Browser

- Go to `http://localhost:5173`
- You need a CIP-30 wallet extension (Typhon, Nami, Eternl, Lace) set to **Preprod**
- Get free testnet ADA: https://docs.cardano.org/cardano-testnet/preprod-faucet

---

## Rules for New Sessions

### DO NOT do these:

1. **DO NOT delete `thirdman.db`** unless you want to lose all users + agreements. The user has existing test data.
2. **DO NOT install Lucid Evolution** (`@lucid-evolution/lucid`). It uses WebAssembly which breaks Vite. All transaction building is done by **Pallas on the backend** in Rust.
3. **DO NOT use `cargo clean`** — it deletes 500+ MB of build cache on C: drive and takes 5+ minutes to rebuild. Use `cargo build` only.
4. **DO NOT kill `terminal64.exe` or `python.exe`** — those are the user's MetaTrader 5 trading bot. They must not be touched.
5. **DO NOT install packages on C: drive** — everything lives on `D:\third-man-app\`.
6. **DO NOT use `Set-Content` with `-Encoding UTF8`** to write `.ts`/`.tsx` files — PowerShell adds a BOM that breaks Vite. Use the Write tool or `[System.IO.File]::WriteAllText()` with `[System.Text.UTF8Encoding]::new($false)`.
7. **DO NOT use `Set-Content` with here-strings (`@"..."@`)** for JSX files — PowerShell mangles `$` and curly braces. Use the Write tool directly.
8. **DO NOT stop processes without asking** — the external D: drive disconnects kill Vite instantly. Always check if the drive is connected first.

### ALWAYS do these:

1. **Check D: drive is connected** before any work:
   ```powershell
   Test-Path "D:\third-man-app\backend\Cargo.toml"
   ```
2. **Close stray terminals** when asked:
   ```powershell
   Get-CimInstance Win32_Process -Filter "Name='powershell.exe'" | Where-Object { $_.ProcessId -ne $PID -and ($_.CommandLine -like '*third-man*' -or $_.CommandLine -like '*vite*') } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
   ```
3. **Keep the DB when restarting** — just restart the `.exe`, don't delete `thirdman.db`.
4. **Use Pallas (Rust backend) for all transaction building** — no Lucid, no Mesh, no browser-side tx building.
5. **Check `reg.txt` on the Desktop** when the user reports errors — they paste error messages there.
6. **Read the `STATUS.md`** for an honest status of what works vs what's stubbed.
7. **Read the `SMART_CONTRACT_SPEC.md`** for the datum/redeemer shapes.
8. **Read the `TX_BUILDER_SPEC.md`** for the Pallas tx builder specification.
9. **Use `cmd /c` for curl commands** — PowerShell mangles JSON escaping.
10. **Backend logs go to stderr** — redirect with `-RedirectStandardError` to capture them.

---

## Project Structure

```
D:\third-man-app\
├── backend/              # Rust + Axum + Pallas + SQLite (port 8080)
│   ├── src/
│   │   ├── main.rs
│   │   ├── config.rs
│   │   ├── crypto.rs          # CIP-8 Ed25519 verification
│   │   ├── error.rs
│   │   ├── state.rs           # AuthUser extractor
│   │   ├── db.rs              # Hashing, UUID
│   │   ├── api/mod.rs         # All routes
│   │   └── modules/
│   │       ├── auth.rs        # Wallet login + DID
│   │       ├── kyc.rs         # Profiles + KYC tiers + privacy
│   │       ├── agreements.rs  # Create / revise / delete / list
│   │       ├── otp.rs         # Expiring invite links
│   │       ├── signing.rs     # CIP-8 agreement signing
│   │       ├── negotiation.rs # Accept-terms state machine
│   │       ├── collateral.rs  # Real Pallas tx for collateral lock
│   │       ├── escrow.rs      # Lock tx + spend tx endpoints
│   │       ├── attachments.rs # File hash storage + proof system
│   │       ├── dispute.rs     # Arbiter + oracle + verdict
│   │       ├── points.rs      # Governance trust points
│   │       ├── receipts.rs    # Receipt anchoring
│   │       ├── ledger.rs      # Immutable push/pull store
│   │       ├── koios.rs       # Koios API provider (UTxOs, submit)
│   │       ├── tx_builder.rs  # Pallas tx builder (lock + spend + assemble)
│   │       └── datum_cbor.rs  # JSON → PlutusData converter + collateral tx
│   ├── migrations/            # SQL migrations (5 files)
│   ├── Cargo.toml             # Includes pallas = "0.30", reqwest
│   └── .env                   # Real script hash + address
│
├── web/                 # React + Vite + TypeScript (port 5173)
│   ├── src/
│   │   ├── main.tsx
│   │   ├── App.tsx             # Router (/, /invite/:code, /arbiter)
│   │   ├── index.css           # Tailwind + dark dApp styles
│   │   ├── lib/
│   │   │   ├── api.ts          # Typed backend client
│   │   │   ├── wallet.ts       # Raw CIP-30 (no Lucid/Mesh)
│   │   │   ├── walletContext.tsx
│   │   │   ├── datum.ts        # Type definitions only (no Lucid)
│   │   │   ├── script.ts       # Script hash + address constants
│   │   │   ├── txBuilder.ts    # Empty stub (backend handles tx building)
│   │   │   └── datumTest.ts    # Empty stub
│   │   ├── components/
│   │   │   ├── Shell.tsx
│   │   │   ├── WalletGate.tsx
│   │   │   ├── StageProfile.tsx
│   │   │   ├── StageForge.tsx
│   │   │   ├── ContractViewer.tsx
│   │   │   ├── StageFlow.tsx
│   │   │   ├── MilestoneDelivery.tsx
│   │   │   ├── ArbiterConsole.tsx
│   │   │   ├── ProtocolRibbon.tsx
│   │   │   ├── GovernancePanel.tsx
│   │   │   └── ProfileEditModal.tsx
│   │   └── pages/
│   │       ├── Journey.tsx
│   │       └── Invite.tsx
│   ├── vite.config.ts          # Proxy config (one origin)
│   └── package.json            # NO @lucid-evolution (removed)
│
├── onchain/             # Aiken smart contract
│   ├── plutus.json     # Compiled blueprint (REAL — hash b8e74f7b...)
│   ├── escrow.ak       # Validator source (written by the user)
│   └── escrow-real.ak  # Backup copy
│
├── package.json        # Root: concurrently
├── README.md           # Full documentation
├── STATUS.md           # Honest status of what works
├── SMART_CONTRACT_SPEC.md  # Datum/redeemer shapes
├── TX_BUILDER_SPEC.md  # Pallas tx builder specification
└── HOW_TO_RUN.md       # THIS FILE
```

---

## Smart Contract

- **Script hash:** `b8e74f7bf6e126055bab145507e59c3bf8fb40059c2239d772ecfe92`
- **Script address (Preprod):** `addr_test1wzuwwnmm7msjvp2m4v292pl9nsal376qqkwzywwhwtk0aysufmxqn`
- **Compiled by:** the user (wrote `escrow.ak` manually, compiled on their own)
- **Validator features:** Deposit, ClaimUnit, SubmitProof, ReviewProof, RaiseDispute, ArbiterResolve, Refund
- **No collateral/slashing** — arbiter fee deducted only inside ArbiterResolve

---

## Transaction Flow (Pallas-based, no Lucid)

```
Frontend                     Backend (Pallas)                    Cardano Preprod
────────                     ────────────────                    ───────────────

1. POST /escrow/init  ──→   Creates DealDatum JSON in DB
                            Converts to PlutusData CBOR

2. GET /escrow/:id/    ──→   Koios: fetch depositor UTxOs
   lock-tx                  Pallas: build unsigned Babbage tx
                            (inputs + script output + change + fee)
                            Returns: { tx_cbor: "hex..." }

3. wallet.signTx       ←──   Frontend receives tx_cbor
   (txCbor, false)          Wallet popup appears
                            User approves
                            Returns: witness CBOR hex

4. POST /escrow/:id/   ──→   assemble_signed_tx(body + witness)
   submit-lock-tx           Koios: POST /submittx (raw CBOR bytes)
                            Returns: { tx_hash: "real_hash" }

5. POST /escrow/:id/   ──→   (For spend txs)
   build-spend-tx           Koios: fetch script UTxO + change UTxOs
                            Pallas: build spend tx
                            Returns: { tx_cbor: "hex..." }

6. wallet.signTx       ←──   Frontend receives tx_cbor
   (txCbor, true)           Wallet signs (partial — script witness added by backend)
                            Returns: witness CBOR hex

7. POST /escrow/:id/   ──→   assemble_signed_tx(body + witness)
   submit-spend-tx          + attach script witness (validator CBOR + redeemer)
                            Koios: POST /submittx
                            Returns: { tx_hash: "real_hash" }
```

---

## Known Issues (as of last session)

1. **Address format** — CIP-30 wallets return hex addresses. Backend needs bech32 for Koios + Pallas. `hex_to_bech32_if_needed()` in `tx_builder.rs` attempts conversion but may fail on some address types.

2. **Datum encoding** — `datum_cbor.rs` converts JSON → PlutusData, but `BoundedBytes` has a 64-byte limit. Long bytearrays (like deal_id) might fail.

3. **No collateral inputs** — Spend txs (ClaimUnit, etc.) need collateral inputs for Plutus execution. `TransactionBody.collateral` is `None`.

4. **No script witness** — The unsigned tx has an empty witness set. For spend txs, the validator script + redeemer must be attached before submission.

5. **Static fee** — `ESTIMATED_FEE = 200_000` is hardcoded. Real fees depend on tx size.

6. **No TTL** — Transactions have no time-to-live. Some wallets require this.

7. **Backend crashes** — `hex_to_bech32_if_needed()` might panic on malformed addresses.

---

## Configuration (.env)

```
DATABASE_URL=sqlite://./thirdman.db?mode=rwc
LISTEN_ADDR=127.0.0.1:8080
ESCROW_VALIDATOR_SCRIPT_HASH=b8e74f7bf6e126055bab145507e59c3bf8fb40059c2239d772ecfe92
ESCROW_VALIDATOR_ADDR=addr_test1wzuwwnmm7msjvp2m4v292pl9nsal376qqkwzywwhwtk0aysufmxqn
ORACLE_ENDPOINT=https://oracle.example.com/query
POINTS_PER_SUCCESS=10
OTP_DEFAULT_TTL_SECONDS=3600
OTP_DEFAULT_MAX_USES=1
COLLATERAL_BASE_LOVELACE=2000000
COLLATERAL_BPS=500
COLLATERAL_MAX_LOVELACE=20000000
```

---

## Testing Two Parties

- Use **two different browsers** (Chrome + Firefox) or one normal + one incognito
- `localStorage` is shared per origin in the same browser — session tokens conflict
- Party 1 (author): forge agreement → generate invite link → copy
- Party 2: open invite link in separate browser → connect wallet → sign in → join → sign

---

## WSL (for Aiken compilation)

- WSL Ubuntu installed on `D:\WSL\`
- Aiken installed at `/usr/local/bin/aiken` in WSL
- Compile script: `D:\third-man-app\compile.sh`
- Run: `wsl -- bash /mnt/d/third-man-app/compile.sh`
- The user wrote `escrow.ak` manually and compiled it themselves
