-- Third Man Protocol schema
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
    id            TEXT PRIMARY KEY,
    did           TEXT NOT NULL UNIQUE,           -- did:cardano:<bech32 addr>
    address       TEXT NOT NULL UNIQUE,           -- bech32 payment address (hex or bech32)
    payment_pubkey TEXT,                          -- hex ed25519 pubkey from CIP-8 COSE_Key
    role          TEXT NOT NULL DEFAULT 'unassigned', -- supplier | buyer | arbiter | unassigned
    status        TEXT NOT NULL DEFAULT 'active', -- active | suspended
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS challenges (
    id          TEXT PRIMARY KEY,
    address     TEXT NOT NULL,
    nonce       TEXT NOT NULL,                    -- hex random nonce to sign with CIP-8 signData
    purpose     TEXT NOT NULL,                    -- register | login
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL,
    used        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS sessions (
    token       TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS kyc_profiles (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    role            TEXT NOT NULL,                -- supplier | buyer
    legal_name      TEXT NOT NULL,
    jurisdiction    TEXT,
    business_type   TEXT,
    contact         TEXT,
    documents_json  TEXT,                         -- hashes/refs of submitted docs
    status          TEXT NOT NULL DEFAULT 'pending', -- pending | verified | rejected
    verified_at     TEXT,
    submitted_at    TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agreements (
    id               TEXT PRIMARY KEY,
    author_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title            TEXT NOT NULL,
    terms_json       TEXT NOT NULL,               -- structured contract terms
    terms_hash       TEXT NOT NULL,               -- blake2b-256 hex of canonical terms
    weight           INTEGER NOT NULL DEFAULT 1,  -- 1..10 severity / value weight
    agreement_value   INTEGER NOT NULL DEFAULT 0, -- lovelace
    collateral_amount INTEGER NOT NULL DEFAULT 0,  -- computed lovelace per participant
    max_participants  INTEGER NOT NULL DEFAULT 2,
    currency_asset   TEXT,                         -- policyid#assetname or "lovelace"
    status           TEXT NOT NULL DEFAULT 'draft', -- draft | negotiating | agreed | locked | active | releasing | completed | disputed | slashed
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agreement_revisions (
    id            TEXT PRIMARY KEY,
    agreement_id  TEXT NOT NULL REFERENCES agreements(id) ON DELETE CASCADE,
    version       INTEGER NOT NULL,
    terms_json    TEXT NOT NULL,
    terms_hash    TEXT NOT NULL,
    proposed_by   TEXT NOT NULL REFERENCES users(id),
    created_at    TEXT NOT NULL,
    UNIQUE(agreement_id, version)
);

CREATE TABLE IF NOT EXISTS otp_links (
    id            TEXT PRIMARY KEY,
    agreement_id TEXT NOT NULL REFERENCES agreements(id) ON DELETE CASCADE,
    code          TEXT NOT NULL UNIQUE,           -- opaque short code
    max_uses      INTEGER NOT NULL,
    uses          INTEGER NOT NULL DEFAULT 0,
    expires_at    TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agreement_participants (
    agreement_id TEXT NOT NULL REFERENCES agreements(id) ON DELETE CASCADE,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role         TEXT NOT NULL,                   -- supplier | buyer
    status       TEXT NOT NULL DEFAULT 'invited', -- invited | joined | signed
    joined_at    TEXT,
    PRIMARY KEY (agreement_id, user_id)
);

CREATE TABLE IF NOT EXISTS agreement_signatures (
    id            TEXT PRIMARY KEY,
    agreement_id  TEXT NOT NULL REFERENCES agreements(id) ON DELETE CASCADE,
    user_id       TEXT NOT NULL REFERENCES users(id),
    terms_hash    TEXT NOT NULL,
    payload_hash  TEXT NOT NULL,                  -- hash of canonical signed payload
    cose_sign1    TEXT NOT NULL,                  -- hex CBOR COSE_Sign1 (CIP-8)
    cose_key      TEXT NOT NULL,                  -- hex CBOR COSE_Key
    verified      INTEGER NOT NULL DEFAULT 0,
    signed_at     TEXT NOT NULL,
    UNIQUE(agreement_id, user_id, terms_hash)
);

CREATE TABLE IF NOT EXISTS smart_contracts (
    id               TEXT PRIMARY KEY,
    agreement_id     TEXT NOT NULL UNIQUE REFERENCES agreements(id) ON DELETE CASCADE,
    validator_hash   TEXT NOT NULL,
    validator_addr   TEXT NOT NULL,
    datum_hash       TEXT,                        -- inline datum hash (CIP-32)
    state            TEXT NOT NULL DEFAULT 'pending', -- pending | locked | releasing | completed | slashed
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS contract_transactions (
    id               TEXT PRIMARY KEY,
    smart_contract_id TEXT NOT NULL REFERENCES smart_contracts(id) ON DELETE CASCADE,
    kind             TEXT NOT NULL,               -- lock | release | slash | refund | anchor
    tx_cbor          TEXT,                        -- hex tx body / witness set (stub for now)
    tx_hash          TEXT,
    status           TEXT NOT NULL DEFAULT 'unsubmitted', -- unsubmitted | submitted | confirmed | failed
    witness_party_ids TEXT,                       -- comma-separated user ids who signed (CIP-30)
    submitted_at     TEXT,
    confirmed_at     TEXT,
    created_at       TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collateral (
    id            TEXT PRIMARY KEY,
    agreement_id TEXT NOT NULL REFERENCES agreements(id) ON DELETE CASCADE,
    user_id      TEXT NOT NULL REFERENCES users(id),
    amount        INTEGER NOT NULL,               -- lovelace
    status        TEXT NOT NULL DEFAULT 'pending', -- pending | locked | returned | slashed
    lock_tx_id    TEXT REFERENCES contract_transactions(id),
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    UNIQUE(agreement_id, user_id)
);

CREATE TABLE IF NOT EXISTS disputes (
    id            TEXT PRIMARY KEY,
    agreement_id TEXT NOT NULL REFERENCES agreements(id) ON DELETE CASCADE,
    raised_by     TEXT NOT NULL REFERENCES users(id),
    reason        TEXT NOT NULL,
    state         TEXT NOT NULL DEFAULT 'open',   -- open | in_review | resolved | closed
    arbiter_id    TEXT REFERENCES users(id),
    verdict       TEXT,                           -- favor_buyer | favor_supplier | split
    rationale     TEXT,
    created_at    TEXT NOT NULL,
    resolved_at   TEXT
);

CREATE TABLE IF NOT EXISTS arbiter_verdicts (
    id           TEXT PRIMARY KEY,
    dispute_id   TEXT NOT NULL REFERENCES disputes(id) ON DELETE CASCADE,
    arbiter_id   TEXT NOT NULL REFERENCES users(id),
    verdict      TEXT NOT NULL,
    rationale    TEXT NOT NULL,
    cose_sign1    TEXT NOT NULL,                   -- CIP-8 signature of verdict by arbiter
    cose_key     TEXT NOT NULL,
    verified     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS arbiters (
    user_id        TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    active         INTEGER NOT NULL DEFAULT 1,
    trust_points   INTEGER NOT NULL DEFAULT 0,
    cases_assigned INTEGER NOT NULL DEFAULT 0,
    cases_resolved INTEGER NOT NULL DEFAULT 0,
    joined_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS oracle_requests (
    id           TEXT PRIMARY KEY,
    dispute_id   TEXT REFERENCES disputes(id) ON DELETE CASCADE,
    source       TEXT NOT NULL,
    query        TEXT NOT NULL,
    result_json  TEXT,
    status       TEXT NOT NULL DEFAULT 'pending', -- pending | fulfilled | failed
    fetched_at   TEXT,
    created_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS receipts (
    id            TEXT PRIMARY KEY,
    contract_id   TEXT NOT NULL REFERENCES smart_contracts(id) ON DELETE CASCADE,
    user_id       TEXT NOT NULL REFERENCES users(id),
    content_hash  TEXT NOT NULL,                  -- blake2b-256 of receipt payload
    content_json  TEXT NOT NULL,
    anchor_tx_hash TEXT,                           -- tx hash anchoring receipt (CIP-10 metadata)
    saved_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS points_ledger (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    delta      INTEGER NOT NULL,                   -- +/- points
    reason     TEXT NOT NULL,
    ref_id     TEXT,                               -- contract/dispute id
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS points_balances (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    balance INTEGER NOT NULL DEFAULT 0
);

-- Immutable ledger mirror: push/pull store. Append-only by design.
CREATE TABLE IF NOT EXISTS ledger_mirror (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    tx_hash      TEXT NOT NULL UNIQUE,             -- on-chain tx hash (or local content hash if pre-submit)
    kind         TEXT NOT NULL,                    -- lock | release | slash | anchor | receipt | dispute_verdict
    ref_id       TEXT,                             -- contract/dispute id
    payload_json TEXT NOT NULL,                    -- canonical snapshot
    content_hash TEXT NOT NULL,                    -- blake2b-256(payload_json)
    block        INTEGER,                          -- block number once confirmed
    confirmed    INTEGER NOT NULL DEFAULT 0,
    pushed_by   TEXT REFERENCES users(id),
    created_at   TEXT NOT NULL,
    confirmed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_challenges_addr ON challenges(address);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_signatures_agr ON agreement_signatures(agreement_id);
CREATE INDEX IF NOT EXISTS idx_participants_agr ON agreement_participants(agreement_id);
CREATE INDEX IF NOT EXISTS idx_disputes_agr ON disputes(agreement_id);
CREATE INDEX IF NOT EXISTS idx_ledger_kind ON ledger_mirror(kind);
CREATE INDEX IF NOT EXISTS idx_ledger_confirmed ON ledger_mirror(confirmed);
