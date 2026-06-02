-- fleet.runner_affinity — the per-zombie lease SLOT. One row per zombie that
-- carries, on a single row, the three things that make multi-runner assignment
-- correct: the atomic claim, the monotonic fencing source, and the sticky hint.
-- The runner never sees this table; zombied owns it.
--
--   * leased_until — the claim. A lease is acquired by a conditional UPSERT
--     that wins iff leased_until < now (slot free or its prior claim expired),
--     so exactly one of N racing runners claims a given zombie. report sets it
--     to the past (slot freed for the next event); a dead runner never frees
--     it, so it expires on its own and another runner re-claims.
--   * fencing_seq — bumped on every claim; it is the lease's fencing_token.
--     Monotonic per zombie, so a reclaim re-lease always carries a strictly
--     higher token and a superseded holder's report is rejected (UZ-RUN-005).
--   * last_runner_id — the sticky-routing hint (which runner last leased this
--     zombie). A preference, never ownership: any eligible runner may claim any
--     zombie. ON DELETE SET NULL drops the hint when the runner is removed, so
--     assignment never blocks on a dead runner.
--
-- fencing_seq + leased_until are set in application code (RULE STS — no static
-- DEFAULT); the first claim seeds fencing_seq = 1.
--
--   * metered_*_tokens/last_metered_at_ms — the DURABLE per-zombie metering
--     cursor. The slot survives a reclaim (the dead holder's lease row is marked
--     expired and a fresh lease row is issued under a higher fencing token, but
--     this row persists), so the cursor here is what lets the re-leased run meter
--     forward from where the dead one stopped. The fenced renewal CTE reads this
--     cursor to compute each /renew's delta and advances it (and the lease-row
--     mirror) atomically. The claim UPSERT seeds it 0/issue-time on a brand-new
--     slot and PRESERVES it on conflict (a reclaim keeps the prior run's value);
--     a fresh event resets it at lease issue.

CREATE TABLE IF NOT EXISTS fleet.runner_affinity (
    id              UUID   PRIMARY KEY,
    CONSTRAINT ck_runner_affinity_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    zombie_id       UUID   NOT NULL,
    last_runner_id  UUID   NULL REFERENCES fleet.runners(id) ON DELETE SET NULL,
    fencing_seq     BIGINT NOT NULL,
    leased_until    BIGINT NOT NULL,
    metered_input_tokens   BIGINT NOT NULL,
    metered_cached_tokens  BIGINT NOT NULL,
    metered_output_tokens  BIGINT NOT NULL,
    last_metered_at_ms     BIGINT NOT NULL,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,
    CONSTRAINT uq_runner_affinity_zombie UNIQUE (zombie_id)
);

-- api_runtime: the serve tier claims the slot (UPSERT) + reads fencing_seq at
-- lease, and releases / reads it at report.
GRANT SELECT, INSERT, UPDATE ON fleet.runner_affinity TO api_runtime;
