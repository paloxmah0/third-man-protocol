# Third Man Protocol

A Cardano escrow protocol for trustless business agreements. Two parties forge a contract, both wallets sign it, funds lock in a smart contract, release on completion — disputes judged by a trust-weighted arbiter pool.

## What This Is

Third Man Protocol is a full-stack dApp that lets suppliers and buyers create binding, on-chain-enforced agreements without trusting each other. The wallet IS the identity. Signatures are cryptographic. Funds are locked in a Plutus validator until conditions are met.

### The Flow

1. **Connect wallet** → CIP-8 nonce signing mints a `did:cardano:<addr>` identity
2. **Build profile** → display name, role types (buyer/supplier), KYC tiers, privacy preferences
3. **Forge agreement** → 7-step wizard: template → describe → add parties → set terms (milestones, deliverables, proof requirements, attachments) → dispute resolution → preview document → save
4. **Invite counterparty** → generate an expiring OTP link, share it
5. **Counterparty joins** → opens the link, connects wallet, picks role
6. **Negotiate** → either party can counter-offer (invalidates all signatures, re-signing required)
7. **Both sign** → CIP-8 message signing of the canonical agreement payload
8. **Lock tx** → author deposits ADA into the Plutus escrow validator (inline datum, CIP-32)
9. **Deliver milestones** → counterparty submits proof (file link + hash), author reviews (accept/reject with mandatory reason, 3 rejections → dispute)
10. **Release tx** → spend the escrow UTxO with a Release redeemer, funds pay out to the recipient
11. **Dispute** (if needed) → raise dispute → arbiter assigned → CIP-8-signed verdict → slash collateral
12. **Receipt** → anchored on the immutable ledger mirror, points awarded for governance

## Project Structure

```
third-man-app/
├── backend/              # Rust gateway (Axum + SQLx + SQLite)
│   ├── src/
│   │   ├── main.rs              # Entry point, server startup
│   │   ├── config.rs            # Environment configuration
│   │   ├── crypto.rs            # CIP-8 COSE_Sign1 parse + Ed25519 verification
│   │   ├── error.rs             # Unified error handling
│   │   ├── state.rs             # AuthUser extractor (Bearer token)
│   │   ├── db.rs                # Hashing, canonical JSON, UUID generation
│   │   ├── api/mod.rs           # All route definitions
│   │   └── modules/
│   │       ├── auth.rs          # Challenge / verify / sessions
│   │       ├── kyc.rs           # Profile + KYC tiers + privacy prefs
│   │       ├── agreements.rs    # Create / revise / delete / list
│   │       ├── otp.rs           # Expiring invite links
│   │       ├── signing.rs       # CIP-8 agreement signing + verification
│   │       ├── negotiation.rs   # Accept-terms state machine
│   │       ├── collateral.rs    # Lock / return / slash
│   │       ├── escrow.rs        # Lock tx + release + dispute (TxBuilder trait)
│   │       ├── attachments.rs   # File hash storage + proof system + milestones
│   │       ├── dispute.rs       # Raise / arbiter / oracle / verdict
│   │       ├── points.rs        # Governance trust points
│   │       ├── receipts.rs      # Receipt anchoring
│   │       └── ledger.rs        # Immutable push/pull store
│   ├── migrations/              # SQL migrations (5 files)
│   ├── Cargo.toml
│   └── .env                     # Configuration
│
├── web/                 # React frontend (Vite + TypeScript + Tailwind)
│   ├── src/
│   │   ├── main.tsx            # React root + providers
│   │   ├── App.tsx             # Router (/, /invite/:code, /arbiter)
│   │   ├── index.css           # Tailwind + custom styles
│   │   ├── lib/
│   │   │   ├── api.ts           # Typed backend client (1:1 with routes)
│   │   │   ├── wallet.ts        # Raw CIP-30 wallet bridge
│   │   │   ├── walletContext.tsx# React context for wallet state
│   │   │   ├── datum.ts         # DealDatum Plutus Data serialization (Lucid)
│   │   │   ├── script.ts        # Blueprint loader + script hash/address
│   │   │   ├── txBuilder.ts     # Lucid tx builder (lock/release/slash/refund/dispute)
│   │   │   └── datumTest.ts     # Round-trip verification
│   │   ├── components/
│   │   │   ├── Shell.tsx              # Header + profile dropdown
│   │   │   ├── WalletGate.tsx         # Connect + CIP-8 login
│   │   │   ├── StageProfile.tsx       # 4-step registration wizard
│   │   │   ├── StageForge.tsx         # 7-step agreement drafting
│   │   │   ├── ContractViewer.tsx     # Contract document (inline + fullscreen)
│   │   │   ├── StageFlow.tsx          # Morphing stage flow (all statuses)
│   │   │   ├── MilestoneDelivery.tsx  # Proof submit/review/bounded resubmission
│   │   │   ├── ArbiterConsole.tsx     # Arbiter dashboard
│   │   │   ├── ProtocolRibbon.tsx     # 8-stage progress tracker
│   │   │   ├── GovernancePanel.tsx    # Points + arbiter pool + receipts
│   │   │   └── ProfileEditModal.tsx   # Edit profile/KYC on demand
│   │   └── pages/
│   │       ├── Journey.tsx     # Dashboard + active agreement view
│   │       └── Invite.tsx      # OTP invite landing page
│   ├── vite.config.ts          # Proxy config (one origin)
│   ├── tailwind.config.js
│   └── package.json
│
├── onchain/             # Aiken smart contract
│   ├── plutus.json     # Compiled blueprint
│   ├── escrow.ak       # Validator source
│   └── escrow-real.ak  # Real logic (with typed datum — to be compiled)
│
├── package.json        # Root: concurrently runs backend + frontend
└── STATUS.md           # Honest status of what works vs what's stubbed
```

## How to Start the Servers

### Prerequisites

- **Rust** (stable, 1.90+) — `rustup install stable`
- **Node.js** (v18+) — https://nodejs.org
- **A CIP-30 wallet** (Typhon, Nami, Eternl, or Lace) installed in your browser
- **Testnet ADA** on Preprod (get free tADA from https://docs.cardano.org/cardano-testnet/preprod-faucet)

### Step 1: Backend

```bash
cd D:\third-man-app\backend

# Copy environment config
copy .env.example .env

# Build and run
cargo run
```

The backend starts on `http://127.0.0.1:8080`.

Verify it's up:
```bash
curl http://127.0.0.1:8080/health
# → {"ok":true,"service":"third-man-backend"}
```

SQLite is created automatically (`thirdman.db`). Migrations run on startup.

### Step 2: Frontend

In a second terminal:

```bash
cd D:\third-man-app\web

# Install dependencies (first time only)
npm install

# Start dev server
npm run dev
```

The frontend starts on `http://localhost:5173`.

### Step 3: Use It

1. Open `http://localhost:5173` in your browser
2. Click **Choose wallet** → select your CIP-30 wallet (Typhon/Nami/Eternl/Lace)
3. Click **Sign in with wallet** → approve the CIP-8 nonce signature in your wallet
4. You're in — forge agreements, invite parties, sign, lock, deliver, release

### Run Both at Once (Optional)

From the root directory:

```bash
cd D:\third-man-app
npm install          # first time only — installs concurrently
npm run dev          # starts both backend + frontend
```

### Configuration (.env)

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `sqlite://./thirdman.db?mode=rwc` | SQLite database path |
| `LISTEN_ADDR` | `127.0.0.1:8080` | Backend listen address |
| `ESCROW_VALIDATOR_SCRIPT_HASH` | placeholder | Plutus script hash (blake2b-224 hex) |
| `ESCROW_VALIDATOR_ADDR` | placeholder | Script address (bech32) |
| `ORACLE_ENDPOINT` | `https://oracle.example.com/query` | Oracle URL (non-fatal) |
| `POINTS_PER_SUCCESS` | `10` | Governance points per completed contract |
| `OTP_DEFAULT_TTL_SECONDS` | `3600` | OTP invite expiry |
| `OTP_DEFAULT_MAX_USES` | `1` | OTP max uses |
| `COLLATERAL_BASE_LOVELACE` | `2000000` | Base collateral (2 ADA) |
| `COLLATERAL_BPS` | `500` | Collateral rate (5% of value) |
| `COLLATERAL_MAX_LOVELACE` | `20000000` | Max collateral (20 ADA) |

### API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/auth/challenge` | Issue CIP-8 nonce |
| POST | `/auth/verify` | Verify signature, mint DID |
| GET | `/auth/me` | Current user |
| POST | `/profile` | Create/update profile |
| GET | `/profile` | Get my profile |
| PATCH | `/profile/privacy` | Update privacy prefs |
| POST | `/kyc/submit` | Submit KYC tier |
| POST | `/kyc/verify` | Verify KYC (operator) |
| POST | `/agreements` | Create agreement |
| GET | `/agreements` | List my agreements |
| GET | `/agreements/:id` | Get agreement |
| DELETE | `/agreements/:id` | Delete draft |
| PATCH | `/agreements/:id/terms` | Revise terms (counter-offer) |
| GET | `/agreements/:id/revisions` | Revision history |
| GET | `/agreements/:id/signable` | Canonical payload for signing |
| POST | `/agreements/:id/sign` | Submit CIP-8 signature |
| POST | `/agreements/:id/accept-terms` | Advance to escrow |
| POST | `/otp` | Create invite link |
| POST | `/otp/redeem` | Join via invite |
| POST | `/collateral/lock` | Lock collateral |
| POST | `/escrow/init` | Initiate escrow |
| GET | `/escrow/:id/lock-tx` | Build lock transaction |
| POST | `/escrow/:id/submit-lock-tx` | Submit signed lock tx |
| POST | `/escrow/:id/complete` | Mark complete |
| POST | `/escrow/:id/release` | Release funds |
| POST | `/disputes` | Raise dispute |
| POST | `/disputes/:id/oracle` | Pull oracle data |
| POST | `/disputes/:id/verdict` | Submit arbiter verdict |
| POST | `/attachments` | Upload attachment (hash + URL) |
| GET | `/attachments` | List attachments |
| POST | `/proofs/submit` | Submit proof for milestone |
| POST | `/proofs/review` | Accept/reject proof |
| GET | `/milestones` | List milestone statuses |
| GET | `/points` | My points balance |
| GET | `/receipts` | My receipts |
| GET | `/ledger` | Pull immutable records |
| POST | `/ledger/push` | Push immutable record |

### Testing Two Parties

For true two-party testing, use **two different browsers** (e.g., Chrome for Party 1, Firefox for Party 2) or one normal + one incognito window. This is because `localStorage` (where the session token lives) is shared per origin in the same browser.

Party 1 (author):
1. Connect wallet → sign in → forge agreement → generate invite link → copy link

Party 2 (counterparty):
1. Open the invite link in a separate browser → connect wallet → sign in → join agreement
2. Sign the agreement with their wallet

Both parties sign → author proceeds to escrow → deposit → milestones → release.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust, Axum, SQLx, SQLite, ed25519-dalek, ciborium, blake2 |
| Frontend | React 18, Vite, TypeScript, Tailwind CSS, Framer Motion, Lucide icons |
| Wallet | Raw CIP-30 (window.cardano), no wrapper library |
| Tx Builder | Lucid Evolution + Koios (Preprod) |
| On-chain | Aiken (Plutus V3) |
| Auth | CIP-8 message signing (COSE_Sign1, Ed25519) |
| Data | Plutus Data CBOR (CIP-32 inline datum) |

## License

MIT
