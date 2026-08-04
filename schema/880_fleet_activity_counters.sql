-- One activity-counter row per fleet, maintained at write time by the triggers
-- in schema/890.
--
-- Why: the fleets list — the Live Wall's hot path — and the single-fleet detail
-- read both need each fleet's lifetime event count and lifetime spend. Computing
-- those by aggregating `core.fleet_events` and the usage ledger on every read is
-- proportional to every child row in the workspace, measured at roughly 1.8
-- seconds for a mature workspace of 100 fleets by 3,000 events. Maintaining the
-- counters at write time makes the read an indexed one-to-one join, constant in
-- history.
--
-- Keyed by its parent, per the pattern stated in
-- `schema/430_tenant_model_selection.sql`. The retired shape carried a generated
-- identity primary key plus `fleet_id UUID NOT NULL UNIQUE`, so the first-touch
-- upsert both triggers perform had two unique indexes to collide on — the exact
-- race the identity work removes, on a row written once per event.
--
-- The counters are monotonic accumulators, never recomputed. `events_processed`
-- counts insertions and `budget_used_nanos` sums charges; both are advanced by
-- deltas rather than by re-aggregating, so neither drifts when history is pruned.

CREATE TABLE IF NOT EXISTS core.fleet_activity_counters (
    fleet_id          UUID   PRIMARY KEY REFERENCES core.fleets(id) ON DELETE CASCADE,
    -- Structural DEFAULTs: zero is the identity of an accumulator, not a
    -- vocabulary value, so these are not the kind RULE STS bans. They let each
    -- trigger's first-touch insert supply only the counter it advances.
    events_processed  BIGINT NOT NULL DEFAULT 0,
    budget_used_nanos BIGINT NOT NULL DEFAULT 0,
    created_at        BIGINT NOT NULL,
    updated_at        BIGINT NOT NULL
);

-- No index. Both triggers and both readers address this table by `fleet_id`,
-- which is the primary key, so the whole access path is already indexed.

-- No uuidv7 CHECK: `fleet_id` is minted by `core.fleets`, whose slot carries the
-- version check.

-- SELECT only, and that is a tightening rather than an oversight. Nothing in the
-- tree writes this table directly — the fleets list and the fleet detail read
-- join it, and every write comes from the triggers in schema/890, which run as
-- their definer. So no runtime role needs INSERT or UPDATE here, and a handler
-- bug cannot corrupt a counter it can only read. The retired slot granted
-- api_runtime all four privileges for writes that never came.
--
-- No DELETE either: rows leave with their fleet through the cascade above.
GRANT SELECT ON core.fleet_activity_counters TO api_runtime;
