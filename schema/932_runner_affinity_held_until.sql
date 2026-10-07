-- A runner holds a fleet's sandbox, frozen, between that fleet's leases.
--
-- `held_until` is the instant, in milliseconds since the epoch, until which
-- the slot's `last_runner_id` keeps the sandbox the fleet's last lease left. A
-- report sets it under the slot's fencing guard; a heartbeat that no longer
-- lists the fleet clears it, and so does another runner's claim (the holder's
-- own claim keeps it). The heartbeat leaves a slot written within the last
-- beat interval alone: the runner lists its holds before its beat goes out,
-- so a report committing in between would otherwise be wiped. The next beat
-- clears it if still unlisted. While it is in the future and the holder can
-- still lease (live, active and not degraded), only the holder claims the
-- fleet, so its next event runs where the last one stopped.
--
-- Nullable, and null means held nowhere: a slot written before this upgrade
-- reads as unheld and claims as it always has. No default and no vocabulary
-- string (RULE STS). The table's existing grants cover the column.
--
-- Forward migration, not a base-statement edit: `VERSION` is past the 0.30.0
-- anchor. Idempotent: a rerun adds nothing.

ALTER TABLE fleet.runner_affinity
    ADD COLUMN IF NOT EXISTS held_until BIGINT;

-- Reader: the heartbeat's hold reconcile (`afd_runner::sql::holds`), which
-- every beat of every runner runs over the slots that runner was last on.
-- Partial, because a slot is held only between a fleet's events, so the index
-- stays the size of the holds rather than of every fleet a runner ever ran.
-- The predicate is a literal in the statement, so a generic plan still
-- matches it.
CREATE INDEX IF NOT EXISTS idx_runner_affinity_last_runner_id_held
    ON fleet.runner_affinity (last_runner_id)
    WHERE held_until IS NOT NULL;
