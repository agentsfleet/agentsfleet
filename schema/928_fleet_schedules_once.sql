-- A schedule may say it fires once.
--
-- A fleet's `schedule` tool asks for one follow-up, not a recurrence: the row
-- fires, and then it retires. Nothing already on the row can carry that.
-- `source_key` is replaced by the external scheduler's own id at the first
-- sync, and `source` says who owns the row, not how long it lives. So the
-- intent is its own column, read by the fire path after it admits the event.
--
-- A boolean, never a defaulted vocabulary string (RULE STS). False is the
-- recurring reading every existing row already has, so no schedule stops
-- firing because of this upgrade.
--
-- Forward migration, not a base-statement edit: `VERSION` is past the 0.30.0
-- anchor, so schema/520 is frozen history. Idempotent: a rerun adds nothing.

ALTER TABLE core.fleet_schedules
    ADD COLUMN IF NOT EXISTS once BOOLEAN NOT NULL DEFAULT false;
