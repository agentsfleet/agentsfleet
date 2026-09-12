-- The physical receipt a lease acknowledges, beside the logical event it ran.
--
-- `event_id` on a lease is the LOGICAL id — the admission ledger's
-- `<created_at>-<seq>`, the value `core.fleet_events`, the usage ledger and
-- every read address. Acknowledging the stream entry needs the PHYSICAL id
-- Redis minted at append, and after a replay the two are different strings:
-- one logical event can have had two entries, and the lease must acknowledge
-- the one it was handed, not the one the ledger last recorded.
--
-- Forward migration, not a base-statement edit: `VERSION` is at the 0.30.0
-- anchor, so schema/610 is frozen history. The backfill is a fact and not a
-- compatibility shim — every lease issued before this slot was issued when
-- the entry id WAS the event id, so its receipt is the value it already holds
-- — and once no NULL remains the column is constrained the way a fresh
-- bootstrap would have written it.

ALTER TABLE fleet.runner_leases ADD COLUMN IF NOT EXISTS receipt TEXT;

UPDATE fleet.runner_leases SET receipt = event_id WHERE receipt IS NULL;

ALTER TABLE fleet.runner_leases ALTER COLUMN receipt SET NOT NULL;
