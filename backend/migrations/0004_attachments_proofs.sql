-- Attachments + proof requirements + bounded resubmission per spec

-- Supporting material attached to the agreement itself (spec sheets, design briefs, etc.)
CREATE TABLE IF NOT EXISTS attachments (
    id              TEXT PRIMARY KEY,
    agreement_id    TEXT NOT NULL REFERENCES agreements(id) ON DELETE CASCADE,
    milestone_index INTEGER,                    -- NULL = agreement-level, 0-based = milestone-specific
    filename        TEXT NOT NULL,
    file_type       TEXT,                        -- document | image | link
    file_size       INTEGER,
    content_hash    TEXT NOT NULL,               -- SHA-256 hex, computed client-side
    uploaded_by     TEXT NOT NULL REFERENCES users(id),
    label           TEXT,                        -- optional: "Photo of completed work"
    purpose         TEXT NOT NULL DEFAULT 'supporting', -- supporting | proof_required
    storage_url     TEXT,                        -- off-chain storage path (never on-chain)
    created_at      TEXT NOT NULL
);

-- Proof submissions for milestone requirements (bounded resubmission)
CREATE TABLE IF NOT EXISTS proof_submissions (
    id              TEXT PRIMARY KEY,
    agreement_id    TEXT NOT NULL REFERENCES agreements(id) ON DELETE CASCADE,
    milestone_index INTEGER NOT NULL,
    attachment_id   TEXT NOT NULL REFERENCES attachments(id) ON DELETE CASCADE,
    attachment_hash TEXT NOT NULL,               -- hash of the submitted file
    submitted_by    TEXT NOT NULL REFERENCES users(id),
    submitted_at    TEXT NOT NULL,
    reviewed_at     TEXT,
    outcome         TEXT NOT NULL DEFAULT 'pending', -- pending | rejected | accepted
    rejection_reason TEXT                        -- mandatory on rejection
);

-- Track rejection count + max attempts per milestone proof requirement
CREATE TABLE IF NOT EXISTS proof_requirements (
    id              TEXT PRIMARY KEY,
    agreement_id    TEXT NOT NULL REFERENCES agreements(id) ON DELETE CASCADE,
    milestone_index INTEGER NOT NULL,
    required        INTEGER NOT NULL DEFAULT 0,
    kind            TEXT,                        -- document | image | link
    label           TEXT,
    max_attempts    INTEGER NOT NULL DEFAULT 3,
    rejection_count INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'pending', -- pending | rejected | accepted | disputed
    UNIQUE(agreement_id, milestone_index)
);
