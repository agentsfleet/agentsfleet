-- A runner holds a fleet's sandbox, frozen, between that fleet's leases.
--
-- `held_until` is the instant, in milliseconds since the epoch, until which
-- the slot's `last_runner_id` keeps the sandbox the fleet's last lease left. A
-- report sets it under the slot's fencing guard; a heartbeat that no longer
-- lists the fleet clears it, and so does any claim. While it is in the future
-- and the holder is live, only the holder claims the fleet, so its next event
-- runs where the last one stopped.
--
-- Nullable, and null means held nowhere: a slot written before this upgrade
-- reads as unheld and claims as it always has. No default and no vocabulary
-- string (RULE STS). The table's existing grants cover the column.
--
-- Forward migration, not a base-statement edit: `VERSION` is past the 0.30.0
-- anchor. Idempotent: a rerun adds nothing.

ALTER TABLE fleet.runner_affinity
    ADD COLUMN IF NOT EXISTS held_until BIGINT;
