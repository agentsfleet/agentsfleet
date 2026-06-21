-- fleet.runner_affinity — the per-fleet lease SLOT. One row per fleet that
-- carries, on a single row, the three things that make multi-runner assignment
-- correct: the atomic claim, the monotonic fencing source, and the sticky hint.
-- The runner never sees this table; agentsfleetd owns it.
--
--   * leased_until — the claim. A lease is acquired by a conditional UPSERT
--     that wins iff leased_until < now (slot free or its prior claim expired),
--     so exactly one of N racing runners claims a given fleet. report sets it
--     to the past (slot freed for the next event); a dead runner never frees
--     it, so it expires on its own and another runner re-claims.
--   * fencing_seq — bumped on every claim; it is the lease's fencing_token.
--     Monotonic per fleet, so a reclaim re-lease always carries a strictly
--     higher token and a superseded holder's report is rejected (UZ-RUN-005).
--   * last_runner_id — the sticky-routing hint (which runner last leased this
--     fleet). A preference, never ownership: any eligible runner may claim any
--     fleet. ON DELETE SET NULL drops the hint when the runner is removed, so
--     assignment never blocks on a dead runner.
--
-- fencing_seq + leased_until are set in application code (RULE STS — no static
-- DEFAULT); the first claim seeds fencing_seq = 1.
--
--   * metered_*_tokens/last_metered_at_ms — the DURABLE per-fleet metering
--     cursor. The slot survives a reclaim (the dead holder's lease row is marked
--     expired and a fresh lease row is issued under a higher fencing token, but
--     this row persists), so the cursor here is what lets the re-leased run meter
--     forward from where the dead one stopped. The fenced renewal CTE reads this
--     cursor to compute each /renew's delta and advances it (and the lease-row
--     mirror) atomically. The claim UPSERT seeds it 0/issue-time on a brand-new
--     slot and PRESERVES it on conflict (a reclaim keeps the prior run's value);
--     a fresh event resets it at lease issue.
--   * meter_slice_seq — the monotonic per-event breakdown counter. The renew /
--     settle CTE derives each fleet.metering_periods.slice_seq from THIS row
--     (+1, written back in the same fenced statement) rather than from a
--     MAX(slice_seq) read of metering_periods: the slot row is FOR UPDATE-locked,
--     so a blocked concurrent renew re-reads the committed counter (EvalPlanQual)
--     and never collides on slice_seq, whereas an unlocked MAX subquery reads a
--     stale statement snapshot and two racing renews pick the same value. Seeded
--     0 on a brand-new slot, PRESERVED on reclaim (the run's slices keep
--     numbering forward), reset to 0 at fresh lease issue.

CREATE TABLE IF NOT EXISTS fleet.runner_affinity (
    uid             UUID   GENERATED ALWAYS AS (id) STORED PRIMARY KEY,
    CONSTRAINT ck_runner_affinity_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    id              UUID   NOT NULL UNIQUE,
    fleet_id       UUID   NOT NULL,
    last_runner_id  UUID   NULL REFERENCES fleet.runners(id) ON DELETE SET NULL,
    fencing_seq     BIGINT NOT NULL,
    leased_until    BIGINT NOT NULL,
    metered_input_tokens   BIGINT NOT NULL,
    metered_cached_tokens  BIGINT NOT NULL,
    metered_output_tokens  BIGINT NOT NULL,
    last_metered_at_ms     BIGINT NOT NULL,
    -- The one DEFAULT in this table: a breakdown counter whose only valid initial
    -- value is 0 (unlike fencing_seq / the cursor, whose seed values are
    -- app-computed and load-bearing). The default keeps the claim UPSERT and every
    -- fixture from re-stating it; the renew/settle CTE always writes it explicitly.
    meter_slice_seq        BIGINT NOT NULL DEFAULT 0,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,
    CONSTRAINT uq_runner_affinity_fleet_id UNIQUE (fleet_id)
);

-- api_runtime: the serve tier claims the slot (UPSERT) + reads fencing_seq at
-- lease, and releases / reads it at report.
GRANT SELECT, INSERT, UPDATE ON fleet.runner_affinity TO api_runtime;
