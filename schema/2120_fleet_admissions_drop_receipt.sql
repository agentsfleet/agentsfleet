-- The ledger claim and report replace physical queue receipts and replay claims.
-- Owner approved these removals; runtime queue readers now use the ledger stamps.
ALTER TABLE fleet.admissions
    DROP COLUMN receipt,
    DROP COLUMN replay_count,
    DROP COLUMN replay_claim_expires_at;

ALTER TABLE runner.leases DROP COLUMN receipt;
