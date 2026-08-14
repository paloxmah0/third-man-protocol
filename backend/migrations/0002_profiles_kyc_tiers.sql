-- Richer registration per spec: Step 2 (basic profile) + Step 3 (tiered KYC) + Step 4 (privacy)

-- Step 2: Basic profile (separate from KYC)
CREATE TABLE IF NOT EXISTS profiles (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    display_name    TEXT NOT NULL,
    avatar_url      TEXT,
    location        TEXT,                          -- "City, Country"
    bio             TEXT,                          -- ~200 chars
    role_types      TEXT NOT NULL DEFAULT '[]',     -- JSON array: ["Developer","Buyer",...]
    languages       TEXT NOT NULL DEFAULT '[]',     -- JSON array
    professional_links TEXT NOT NULL DEFAULT '[]',  -- JSON: [{type,url,visible}]
    settlement_rails TEXT NOT NULL DEFAULT '[]',    -- JSON: ["ADA","stablecoin","M-Pesa","mixed"]
    deal_size_range TEXT,                          -- "<$100" | "$100-1k" | "$1k-10k" | "$10k+"
    availability    TEXT,                          -- self-declared response time expectation
    -- Organization mode (optional)
    org_name        TEXT,
    org_type        TEXT,                          -- DAO | Registered entity | Informal collective | Solo
    org_members     TEXT NOT NULL DEFAULT '[]',     -- JSON array of wallet addresses
    -- Verification signals (lightweight, separate from KYC)
    verified_signals TEXT NOT NULL DEFAULT '[]',    -- JSON: [{type:"github",verified:true}, ...]
    -- Privacy preferences (Step 4) — JSON map of field→visibility
    privacy_prefs   TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- Step 3: Tiered KYC (optional, non-blocking)
-- Tier 0: wallet only (default)
-- Tier 1: phone + OTP
-- Tier 2: ID/passport + selfie
CREATE TABLE IF NOT EXISTS kyc_tiers (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    tier            INTEGER NOT NULL DEFAULT 0,    -- 0 | 1 | 2
    -- Tier 1 fields
    phone           TEXT,
    phone_verified  INTEGER NOT NULL DEFAULT 0,
    -- Tier 2 fields
    legal_name      TEXT,
    document_type   TEXT,                          -- passport | national_id | drivers_license
    document_hash   TEXT,                          -- hash of submitted doc (stored off-chain, hash only)
    selfie_hash     TEXT,                          -- hash of selfie match proof
    -- Attestation (on-chain commitment)
    attestation_hash TEXT,                          -- hash committed on-chain
    attestor_id     TEXT,                           -- who attested
    issued_at       TEXT,
    expiry_at       TEXT,
    status          TEXT NOT NULL DEFAULT 'none',   -- none | pending_t1 | pending_t2 | verified_t1 | verified_t2 | rejected
    submitted_at    TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- Privacy defaults applied at registration
INSERT OR IGNORE INTO profiles (id, user_id, display_name, privacy_prefs, created_at, updated_at)
SELECT 'seed-' || u.id, u.id, substr(u.did, 1, 20),
'{"display_name":"public","avatar":"public","location":"public_country","bio":"public","role_types":"public","languages":"public","professional_links":"private","deal_size_range":"participants_only","settlement_rails":"participants_only","org_members":"private","verified_signals":"public","kyc_tier":"public","reputation":"public","phone":"participants_only","email":"participants_only","deal_history":"private"}',
strftime('%Y-%m-%dT%H:%M:%SZ','now'), strftime('%Y-%m-%dT%H:%M:%SZ','now')
FROM users u
WHERE NOT EXISTS (SELECT 1 FROM profiles p WHERE p.user_id = u.id);

-- Seed KYC tier rows for existing users
INSERT OR IGNORE INTO kyc_tiers (id, user_id, tier, status, submitted_at, updated_at)
SELECT 'seed-kyc-' || u.id, u.id, 0, 'none', strftime('%Y-%m-%dT%H:%M:%SZ','now'), strftime('%Y-%m-%dT%H:%M:%SZ','now')
FROM users u
WHERE NOT EXISTS (SELECT 1 FROM kyc_tiers k WHERE k.user_id = u.id);
