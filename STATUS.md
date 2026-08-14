# Third Man Protocol — Honest Status

## What WORKS (off-chain, fully functional)

### Backend (Rust/Axum)
- ✅ Wallet connect + CIP-8 nonce signing → DID mint
- ✅ Registration: 4-step profile wizard (profile → KYC tiers → privacy prefs)
- ✅ Agreement drafting: 7-step wizard (template → describe → parties → terms → disputes → preview → send)
- ✅ OTP invite links with expiry + capacity
- ✅ Counter-offer flow (Party 2 can counter, Party 1 can re-counter after receiving one)
- ✅ CIP-8 agreement signing by both wallets
- ✅ Collateral locking (off-chain record)
- ✅ Milestone tracking with deliverables + proof requirements
- ✅ Proof submission (link + hash) → author review (accept/reject with mandatory reason)
- ✅ Bounded resubmission (3 rejections → disputed)
- ✅ Dispute raising → arbiter assignment → CIP-8 verdict → slash
- ✅ Points/governance system
- ✅ Immutable ledger mirror (push/pull)
- ✅ Receipts

### Frontend (React/Vite/TypeScript)
- ✅ Bold dark dApp UI with aurora background, glassmorphism
- ✅ Wallet picker (auto-detects CIP-30 wallets)
- ✅ Protocol ribbon (8-stage progress tracker)
- ✅ Contract document viewer (serif font, numbered sections, inline + fullscreen)
- ✅ Profile edit modal (accessible from avatar dropdown)
- ✅ Arbiter console (enroll, view disputes, evidence trail, submit verdict)
- ✅ Delete draft agreements (with "agreement deleted" message for Party 2)
- ✅ Attachment visibility (file links in contract viewer + review panel)

## What DOES NOT WORK (on-chain, stubbed)

### Smart Contract
- ❌ **NO working smart contract exists.** The Aiken validator is a placeholder (`todo`) that always fails.
- ❌ The real escrow logic (Deposit/Release/Slash/Refund/Dispute) is written in `onchain/escrow.ak` but **cannot be compiled** — Aiken v1.1.23 has a bug where custom typed datum/redeemer in the `spend` handler causes a silent build failure on both Windows and WSL.
- ❌ No real ADA moves anywhere. The "Deposit & activate" button only creates a database record.

### Transaction Building
- ❌ Lucid Evolution tx builder is wired but fails because:
  - The script address points to a placeholder validator
  - The wallet may not be properly initialized for Preprod
  - The stub CBOR from the backend isn't valid Cardano transaction format
- ❌ The "Sign release" button fails because the placeholder validator rejects all spends

### What needs to happen for real on-chain functionality
1. **Compile the real Aiken validator** — either:
   - Wait for an Aiken compiler fix (v1.1.24+)
   - Write the validator in raw Plutus Core / OpShin / op-shin
   - Use a multisig script as an interim (simpler, no Plutus needed)
2. **Deploy the validator to Preprod** — one-time reference script tx
3. **Wire Lucid to build real transactions** with the real script hash + address
4. **Test with real testnet ADA**

## Architecture (what's built)

```
D:\third-man-app\
├── backend/          # Rust/Axum gateway (port 8080)
│   ├── src/
│   │   ├── main.rs
│   │   ├── config.rs
│   │   ├── crypto.rs          # CIP-8 verification (REAL)
│   │   ├── api/mod.rs         # All routes
│   │   ├── modules/
│   │   │   ├── auth.rs        # Wallet login + DID
│   │   │   ├── kyc.rs         # Profiles + KYC tiers + privacy
│   │   │   ├── agreements.rs  # Drafting + revisions + delete
│   │   │   ├── otp.rs         # Expiring invites
│   │   │   ├── signing.rs     # CIP-8 agreement signing
│   │   │   ├── negotiation.rs # Accept-terms state machine
│   │   │   ├── collateral.rs  # Lock/return/slash
│   │   │   ├── escrow.rs      # Lock tx + release + dispute (STUBBED tx builder)
│   │   │   ├── attachments.rs # File hash storage + proof system
│   │   │   ├── dispute.rs     # Arbiter + oracle + verdict
│   │   │   ├── points.rs      # Governance trust points
│   │   │   ├── receipts.rs    # Receipt anchoring
│   │   │   └── ledger.rs      # Immutable push/pull store
│   │   └── migrations/        # 5 SQL migrations
│   └── .env                   # Script hash = placeholder
│
├── web/              # React/Vite/TypeScript (port 5173)
│   ├── src/
│   │   ├── lib/
│   │   │   ├── api.ts          # Typed backend client
│   │   │   ├── wallet.ts       # Raw CIP-30 (no Mesh)
│   │   │   ├── walletContext.tsx
│   │   │   ├── datum.ts        # Plutus Data serialization (Lucid)
│   │   │   ├── script.ts       # Blueprint loader
│   │   │   ├── txBuilder.ts    # Lucid tx builder (STUBBED)
│   │   │   └── datumTest.ts    # Round-trip verification
│   │   ├── components/
│   │   │   ├── Shell.tsx              # Header + profile dropdown
│   │   │   ├── WalletGate.tsx         # Connect + CIP-8 login
│   │   │   ├── StageProfile.tsx       # 4-step registration wizard
│   │   │   ├── StageForge.tsx         # 7-step agreement drafting
│   │   │   ├── ContractViewer.tsx     # Contract document (inline + fullscreen)
│   │   │   ├── StageFlow.tsx          # Morphing stage flow
│   │   │   ├── MilestoneDelivery.tsx  # Proof submit/review
│   │   │   ├── ArbiterConsole.tsx     # Arbiter UI
│   │   │   ├── ProtocolRibbon.tsx     # 8-stage progress
│   │   │   ├── GovernancePanel.tsx    # Points + arbiters + receipts
│   │   │   └── ProfileEditModal.tsx
│   │   └── pages/
│   │       ├── Journey.tsx     # Dashboard + active agreement
│   │       └── Invite.tsx      # OTP invite landing
│   └── vite.config.ts          # Proxy to backend
│
├── onchain/          # Aiken validator
│   ├── plutus.json   # Compiled blueprint (PLACEHOLDER)
│   ├── escrow.ak     # Validator source (REAL logic, won't compile)
│   └── escrow-real.ak # Copy with real types
│
└── package.json      # Root: npm run dev (concurrently)
```

## The Aiken compiler bug

The Aiken v1.1.23 compiler silently fails to emit `plutus.json` when:
- Custom type definitions (e.g., `type DealDatum { ... }`) are used in the `spend` handler signature
- The `Constr` pattern matching is used in `when` expressions

This happens on **both Windows and WSL Ubuntu**. The compiler reports no errors (`aiken check` passes), but `aiken build` produces no output.

**Workarounds:**
1. Use `Data` types (untyped) — works but means the validator can't type-check the datum
2. Wait for Aiken v1.1.24+ fix
3. Write the validator in OpShin (Python → Plutus) or raw Plutus Core
4. Use a Cardano multisig script as interim (no Plutus needed, simpler)
