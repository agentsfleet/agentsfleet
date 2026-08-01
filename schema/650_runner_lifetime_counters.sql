-- One lifetime tally row per runner, maintained by the lease write paths.
--
-- Why: the runner detail read aggregated the runner's whole lease history — plus
-- a per-lease join to `core.fleet_events` for outcome classification — on every
-- page load. That read grows with the runner's history; a mature runner holds
-- tens of thousands of leases. Maintaining the tallies at write time makes the
-- detail read an indexed one-to-one join, constant in history.
--
-- No triggers, unlike the activity counters in schema/890: classifying a terminal
-- transition needs the lease and event status vocabulary, which lives in
-- application constants, and RULE STS keeps vocabularies out of schema objects so
-- they cannot drift from code. Instead each transition's owning statement carries
-- a counter arm conditioned on the guarded write actually affecting rows — single
-- owner per transition, transactional by construction, retry-safe.
--
-- Keyed by its parent, per the pattern stated in
-- `schema/430_tenant_model_selection.sql`. This is the table that first recorded
-- the reason, before it was general: it deliberately shipped exactly ONE unique
-- index because a second breaks concurrent first-touch upserts — `ON CONFLICT`
-- arbitrates exactly one constraint, so two sessions inserting a brand-new
-- runner's row race to a duplicate-key error on the other index instead of taking
-- the update arm. Under the frozen-slot model it could not drop the duplicate
-- column, so it kept the generated twin as the primary key and added a CHECK
-- tying it equal to `runner_id`. Rebuilt from empty, the column simply goes.
--
-- The counters are monotonic and only ever increase. They count TRANSITIONS, not
-- surviving rows, so they stay correct after the retention sweep deletes a
-- runner's aged lease history — which is also why no reconciler recounts them on
-- a schedule: after the first prune a recount is no longer a source of truth.

CREATE TABLE IF NOT EXISTS fleet.runner_lifetime_counters (
    runner_id  UUID   PRIMARY KEY REFERENCES fleet.runners(id) ON DELETE CASCADE,
    -- Structural DEFAULTs: zero is the only valid initial value for a tally, an
    -- identity rather than a vocabulary value, so these are not the kind RULE STS
    -- bans. They let the first-touch upsert insert without restating four zeroes.
    acquired   BIGINT NOT NULL DEFAULT 0,
    succeeded  BIGINT NOT NULL DEFAULT 0,
    failed     BIGINT NOT NULL DEFAULT 0,
    expired    BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

-- No backfill statement. The retired slot carried one, with a GREATEST conflict
-- arm that made it safe to re-run at any age, because it was landing on a
-- populated database during a rolling deploy: leases acquired by a not-yet-
-- replaced replica were counted by nobody, and the tallies sat permanently low by
-- however many the rollout overlapped. A database bootstrapped from empty has no
-- history to recover, so the statement would sum zero rows for zero runners.
-- If this table is ever re-introduced onto a populated deployment, that backfill
-- and its GREATEST arm are the thing to bring back — an absolute assignment would
-- silently zero a mature runner's totals once retention had pruned its history.

-- No index. Every read and the upsert address this table by `runner_id`, which
-- is the primary key, so the whole access path is already indexed.

-- No uuidv7 CHECK: `runner_id` is minted by `fleet.runners`, whose slot carries
-- the version check.

GRANT SELECT, INSERT, UPDATE ON fleet.runner_lifetime_counters TO api_runtime;

-- metering_runtime: the settle statement carries a tally arm that first-touch
-- upserts this row, so it needs INSERT as well as UPDATE (schema/120).
GRANT SELECT, INSERT, UPDATE ON fleet.runner_lifetime_counters TO metering_runtime;
