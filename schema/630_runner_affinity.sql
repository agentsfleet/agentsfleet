-- The per-fleet lease SLOT: one row per fleet carrying, on a single row, the
-- three things that make multi-runner assignment correct — the atomic claim, the
-- monotonic fencing source, and the sticky hint. The runner never sees this
-- table; agentsfleetd owns it.
--
--   leased_until    the claim. A lease is acquired by a conditional upsert that
--                   wins if and only if leased_until < now (the slot is free, or
--                   its prior claim expired), so exactly one of N racing runners
--                   claims a given fleet. Report sets it to the past, freeing the
--                   slot for the next event; a dead runner never frees it, so it
--                   expires on its own and another runner re-claims.
--   fencing_seq     bumped on every claim; it is the lease's fencing token.
--                   Monotonic per fleet, so a reclaim re-lease always carries a
--                   strictly higher token and a superseded holder's report is
--                   rejected (UZ-RUN-005).
--   last_runner_id  the sticky-routing hint — which runner last leased this
--                   fleet. A preference, never ownership: any eligible runner may
--                   claim any fleet. ON DELETE SET NULL drops the hint when the
--                   runner is removed, so assignment never blocks on a dead one.
--
-- `fencing_seq` and `leased_until` are seeded in application code, never by a
-- schema DEFAULT: both seed values are computed and load-bearing (the first claim
-- seeds fencing_seq = 1).
--
-- The metering cursor (metered_*_tokens, last_metered_at) is the DURABLE
-- per-fleet one. The slot survives a reclaim — the dead holder's lease row is
-- marked expired and a fresh lease is issued under a higher fencing token, but
-- this row persists — so the cursor here is what lets the re-leased run meter
-- forward from where the dead one stopped. The fenced renewal statement reads
-- this cursor to compute each renewal's delta and advances it, and the lease-row
-- mirror, atomically. The claim upsert seeds it at zero and issue-time on a
-- brand-new slot and PRESERVES it on conflict, so a reclaim keeps the prior run's
-- value; a fresh event resets it at lease issue.
--
-- `meter_slice_seq` is gone. It existed only to number the per-renewal rows of
-- `fleet.metering_periods`, and that table is removed rather than carried: it was
-- derived data no product surface read. Nothing else ever read the
-- counter, so it leaves with its only consumer.
--
-- Keyed by its parent, per the pattern stated in
-- `schema/430_tenant_model_selection.sql`, and this is the table where it matters
-- most. The slot is claimed by an upsert that arbitrates ON CONFLICT (fleet_id)
-- on the hottest write path in the system; the retired shape carried a generated
-- identity column, a unique `id`, AND a separate `UNIQUE (fleet_id)`, so two racing
-- to claim a brand-new fleet's slot could collide on an index the statement did
-- not name and fail with a duplicate-key error instead of taking the update arm.
-- With `fleet_id` as the primary key there is exactly one unique index and the
-- conflict target IS it.

CREATE TABLE IF NOT EXISTS fleet.runner_affinity (
    fleet_id              UUID   PRIMARY KEY REFERENCES core.fleets(id) ON DELETE CASCADE,
    last_runner_id        UUID   REFERENCES fleet.runners(id) ON DELETE SET NULL,
    fencing_seq           BIGINT NOT NULL,
    leased_until          BIGINT NOT NULL,
    metered_input_tokens  BIGINT NOT NULL,
    metered_cached_tokens BIGINT NOT NULL,
    metered_output_tokens BIGINT NOT NULL,
    last_metered_at       BIGINT NOT NULL,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL
);

-- Reader: the liveness sweeper's expireActiveLeaseSlots, which filters by
-- last_runner_id once PER DUE RUNNER PER SWEEP CYCLE — so its cost is fleets x
-- runners x cycles, the only multiplicative read in the sweep, and this table
-- grows with fleet count independently of the runner population.
-- `last_runner_id` is also a foreign key with a referential action, so without
-- this index deleting a runner scanned the table too. `leased_until` carries the
-- range predicate in the same statement.
CREATE INDEX IF NOT EXISTS idx_runner_affinity_last_runner_id_leased_until
    ON fleet.runner_affinity (last_runner_id, leased_until);

-- api_runtime: the serve tier claims the slot (upsert) and reads fencing_seq at
-- lease, then releases and reads it at report.
GRANT SELECT, INSERT, UPDATE ON fleet.runner_affinity TO api_runtime;

-- metering_runtime: the fenced statement locks this slot `FOR UPDATE` to
-- serialise a racing reclaim, and advances the metering cursor on it in the same
-- statement that debits the wallet (schema/120). That cursor advance is what
-- makes a replayed renewal charge nothing, so it cannot be split out. No INSERT:
-- the slot is created by the claim upsert, which does not elevate.
GRANT SELECT, UPDATE ON fleet.runner_affinity TO metering_runtime;
