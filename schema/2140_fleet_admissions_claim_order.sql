-- Claims follow the serialized identity sequence, not transaction-start time.
-- The fleet-row lock is held before INSERT through commit, so this order follows
-- each fleet's serialized admission path.
DROP INDEX fleet.idx_admissions_unsettled;

CREATE INDEX idx_admissions_unsettled
    ON fleet.admissions (fleet_id, sequence)
    WHERE settled_at IS NULL;
