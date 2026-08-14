-- Contract document structure per spec: recitals, scope, milestones, obligations, release conditions

-- We store the full contract document as structured JSON in terms_json, so no schema
-- change to the agreements table itself is needed. But we add columns for key indexed fields.

ALTER TABLE agreements ADD COLUMN release_condition TEXT DEFAULT 'mutual_confirm';
-- mutual_confirm | oracle | timeout_to_dispute | hybrid_arbiter

ALTER TABLE agreements ADD COLUMN dispute_window_days INTEGER DEFAULT 7;

ALTER TABLE agreements ADD COLUMN arbiter_fee_percent INTEGER DEFAULT 0;
ALTER TABLE agreements ADD COLUMN arbiter_fee_paid_by TEXT DEFAULT 'party1';
