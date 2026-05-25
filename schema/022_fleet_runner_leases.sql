-- fleet.runner_leases — one row per issued lease (zombie-runner split). The
-- control plane (zombied) records a lease when it hands an event to a runner
-- via POST /v1/runners/me/leases, and reads it back at
-- POST /v1/runners/me/reports to reconstruct the write context the direct
-- worker holds in memory. The runner never sees this table; it only echoes the
-- opaque lease id and fencing_token.
--
-- Why these columns: the report handler reproduces the direct worker's
-- finalize() — markTerminal (zombie_id, event_id), recordStageActuals
-- (tenant_id + workspace_id, zombie_id, event_id, posture, model) and
-- checkpointZombieSession (zombie_id). The lease persists exactly that context
-- so report rebuilds it without re-resolving the tenant/provider.
--
-- fencing_token: monotonic per lease; frozen here for the later fleet reclaim
--   logic (lease_expires_at + fencing rejection). The control plane records it
--   but does not verify it yet — the single-zombie loopback skeleton has no
--   reclaim path to exercise.
-- status: lease lifecycle — app-enforced values (active | reported | expired),
--   no static-string CHECK (RULE STS).
-- posture/model: the metering posture + model resolved at lease, replayed into
--   recordStageActuals at report.

CREATE TABLE IF NOT EXISTS fleet.runner_leases (
    id                UUID   PRIMARY KEY,
    CONSTRAINT ck_runner_leases_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    runner_id         UUID   NOT NULL REFERENCES fleet.runners(id) ON DELETE CASCADE,
    zombie_id         UUID   NOT NULL,
    workspace_id      UUID   NOT NULL,
    tenant_id         UUID   NOT NULL,
    event_id          TEXT   NOT NULL,
    posture           TEXT   NOT NULL,
    model             TEXT   NOT NULL,
    fencing_token     BIGINT NOT NULL,
    lease_expires_at  BIGINT NOT NULL,
    status            TEXT   NOT NULL,
    created_at        BIGINT NOT NULL,
    updated_at        BIGINT NOT NULL
);

-- api_runtime: the serve tier owns /v1/runners/me/{leases,reports}; it inserts
-- a lease at lease-issue and reads + updates status at report.
GRANT SELECT, INSERT, UPDATE ON fleet.runner_leases TO api_runtime;
