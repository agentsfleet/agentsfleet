-- Admissions retain the queue state beside the accepted event. The development
-- cutover resets fleet data before starting ledger readers; no history backfill.
ALTER TABLE fleet.admissions
    ADD COLUMN lease_fence BIGINT,
    ADD COLUMN parked_at TIMESTAMPTZ(3),
    ADD COLUMN settled_at TIMESTAMPTZ(3);

CREATE INDEX idx_admissions_unsettled
    ON fleet.admissions (fleet_id, created_at, sequence)
    WHERE settled_at IS NULL;

GRANT SELECT, INSERT, UPDATE ON fleet.admissions TO api_runtime;
