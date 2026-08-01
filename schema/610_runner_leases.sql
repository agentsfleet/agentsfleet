-- One row per issued lease. The control plane records a lease when it hands an
-- event to a runner via POST /v1/runners/me/leases, and reads it back at
-- POST /v1/runners/me/reports to reconstruct the write context. The runner never
-- sees this table; it only echoes the opaque lease id and fencing token.
--
-- Why these columns: the report handler reproduces the direct worker's
-- finalize() — markTerminal (fleet_id, event_id), recordStageActuals
-- (tenant_id + workspace_id, fleet_id, event_id, posture, model) and
-- checkpointFleetSession (fleet_id). The lease persists exactly that context so
-- report rebuilds it without re-resolving the tenant or provider.
--
-- The lease carries NO copy of the event body. It used to hold `request_json`,
-- a second full copy of the same payload the event row already stores, written
-- on every lease issue for the sole benefit of reclaim. Reclaim now joins
-- `core.fleet_events` on (fleet_id, event_id) to read the body instead: both
-- tables cascade from the same fleet, so the join cannot dangle, and the write
-- path stops duplicating the largest value in the system.
-- `actor`, `event_type` and `event_created_at` stay — they are small scalars the
-- report path reads per lease, not payload.
--
-- `workspace_id` and `tenant_id` are denormalised copies carried so the report
-- and settle paths need no join, and they are constrained TOGETHER with
-- `fleet_id` by the composite foreign key below. That is a money guarantee, not
-- tidiness: the settle statement locks the wallet it finds through this row's
-- `tenant_id`, so an unconstrained copy would let a lease-issue bug debit
-- another tenant's balance and record the charge as legitimate. Referencing all
-- three columns against the fleet's own makes that impossible to write, and
-- costs nothing at settle because no extra join appears in the fenced statement.
--
--   fencing_token          monotonic per lease. A reclaim re-lease always
--                          carries a strictly higher token, so a superseded
--                          holder's report is rejected (UZ-RUN-005).
--   status                 lease lifecycle: active | reported | expired.
--                          App-enforced vocabulary, no SQL CHECK (RULE STS).
--   posture/provider/model the metering posture, the resolved provider, and the
--                          model resolved at lease. Provider and model together
--                          key the rate row — the same model under two providers
--                          prices apart — and are replayed into the renew credit
--                          gate and the report settle.
--   metered_*_tokens       the incremental-metering cursor. Each renewal charges
--   last_metered_at        the difference between the runner's cumulative token
--                          counts and these last-metered values, plus the run fee
--                          for the elapsed span, then advances the cursor in the
--                          SAME fenced statement, so a re-sent renewal
--                          double-bills nothing. Initialised at lease issue and
--                          carried forward on reclaim, so the re-leased holder
--                          meters from where the dead one stopped — no double
--                          charge, no gap.

CREATE TABLE IF NOT EXISTS fleet.runner_leases (
    id                    UUID   PRIMARY KEY,
    CONSTRAINT ck_runner_leases_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    runner_id             UUID   NOT NULL REFERENCES fleet.runners(id) ON DELETE CASCADE,
    fleet_id              UUID   NOT NULL,
    workspace_id          UUID   NOT NULL,
    tenant_id             UUID   NOT NULL,
    event_id              TEXT   NOT NULL,
    actor                 TEXT   NOT NULL,
    event_type            TEXT   NOT NULL,
    event_created_at      BIGINT NOT NULL,
    posture               TEXT   NOT NULL,
    provider              TEXT   NOT NULL,
    model                 TEXT   NOT NULL,
    metered_input_tokens  BIGINT NOT NULL,
    metered_cached_tokens BIGINT NOT NULL,
    metered_output_tokens BIGINT NOT NULL,
    last_metered_at       BIGINT NOT NULL,
    fencing_token         BIGINT NOT NULL,
    lease_expires_at      BIGINT NOT NULL,
    status                TEXT   NOT NULL,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL,
    -- The fleet edge, carrying the scope with it. Cascades on fleet deletion
    -- exactly as the single-column reference it replaces did; the index leading
    -- with `fleet_id` in schema/620 serves that cascade.
    CONSTRAINT fk_runner_leases_fleet_id_workspace_id_tenant_id
        FOREIGN KEY (fleet_id, workspace_id, tenant_id)
        REFERENCES core.fleets (id, workspace_id, tenant_id) ON DELETE CASCADE
);

-- Indexes live in schema/620_runner_lease_indexes.sql: there are five, each
-- carrying its own measured evidence, and they read as their own concern.

-- api_runtime: the serve tier owns /v1/runners/me/{leases,reports}; it inserts a
-- lease at issue and reads and updates status at report. DELETE is the retention
-- sweeper's (fleet/retention_sweeper.zig), which removes terminal-status rows
-- older than the retention window — the one retention policy that exists, kept
-- exactly as M149 shipped it.
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.runner_leases TO api_runtime;

-- metering_runtime: the fenced settle/renewal statement locks this row
-- `FOR UPDATE` and flips its status in the same statement that moves the wallet
-- (schema/120). No INSERT — a lease is born at issue, unelevated — and no
-- DELETE, which belongs to the retention sweeper alone.
GRANT SELECT, UPDATE ON fleet.runner_leases TO metering_runtime;
