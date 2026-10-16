-- The session row is the resume bookmark, and only that.
--
-- `execution_id` and `execution_started_at` (slot 510) had no writer of a value
-- and no reader: which fleet is executing right now is `fleet.runner_leases`,
-- which carries the fence and the expiry that make the answer trustworthy.
--
-- Owner decision, Indy, Oct 15, 2026. VERSION is 0.61.0, above the
-- 0.30.0 anchor, so slot 510 stays frozen history. No index, view or trigger
-- names either column.

ALTER TABLE core.fleet_sessions
    DROP COLUMN IF EXISTS execution_id,
    DROP COLUMN IF EXISTS execution_started_at;
