-- A one-off schedule remembers the instant it was meant for.
--
-- A `once` schedule is an expression with no year, because the external
-- scheduler takes no other kind. When it cannot be registered until after its
-- minute has passed, a later sync would register the same expression, and its
-- next match is a year away. The intended instant, in milliseconds since the
-- epoch, lets that sync retire the schedule instead.
--
-- Nullable: a recurring schedule has no single instant, and a `once` row
-- written before this upgrade keeps firing as it would have. No default and no
-- vocabulary string (RULE STS). The table's existing grants cover the column.
--
-- Forward migration, not a base-statement edit: `VERSION` is past the 0.30.0
-- anchor. Idempotent: a rerun adds nothing.

ALTER TABLE core.fleet_schedules
    ADD COLUMN IF NOT EXISTS fire_at BIGINT;
