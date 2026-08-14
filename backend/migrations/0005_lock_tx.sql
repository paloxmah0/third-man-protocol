-- Lock transaction support: funding deadline, multi-depositor tracking, DealDatum

ALTER TABLE smart_contracts ADD COLUMN funding_deadline TEXT;
ALTER TABLE smart_contracts ADD COLUMN funded_so_far INTEGER NOT NULL DEFAULT 0;
ALTER TABLE smart_contracts ADD COLUMN total_required INTEGER NOT NULL DEFAULT 0;
ALTER TABLE smart_contracts ADD COLUMN deal_datum_json TEXT;

-- Track individual funding contributions (multi-depositor)
CREATE TABLE IF NOT EXISTS funding_contributions (
    id               TEXT PRIMARY KEY,
    smart_contract_id TEXT NOT NULL REFERENCES smart_contracts(id) ON DELETE CASCADE,
    user_id          TEXT NOT NULL REFERENCES users(id),
    amount           INTEGER NOT NULL,
    tx_hash          TEXT,
    witness          TEXT,
    status           TEXT NOT NULL DEFAULT 'pending', -- pending | signed | submitted | confirmed | failed
    created_at       TEXT NOT NULL,
    confirmed_at     TEXT
);
