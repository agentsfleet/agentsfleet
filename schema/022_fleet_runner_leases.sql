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
-- Event envelope (actor, event_type, request_json, event_created_at): the
-- runner's input event, persisted so an expired lease can be RE-LEASED to
-- another runner from Postgres alone (the durable reclaim source) without
-- re-reading Redis — durable state lives in zombied, never runner-local. Not
-- credentials: secrets are resolved fresh per lease, never stored here (VLT).
--
-- fencing_token: monotonic per lease; frozen here for the later fleet reclaim
--   logic (lease_expires_at + fencing rejection). The control plane records it
--   but does not verify it yet — the single-zombie loopback skeleton has no
--   reclaim path to exercise.
-- status: lease lifecycle — app-enforced values (active | reported | expired),
--   no static-string CHECK (RULE STS).
-- posture/provider/model: the metering posture, the resolved provider name, and
--   the model resolved at lease. provider + model together key the rate row
--   (the same model under two providers prices apart), replayed into the renew
--   credit gate + the report settle.
-- metered_*_tokens/last_metered_at_ms: the incremental-metering cursor. Each
--   /renew charges the diff between the runner's cumulative token counts and
--   these last-metered values (plus the run fee for now - last_metered_at_ms),
--   then advances the cursor in the SAME fenced CTE, so a re-sent renewal
--   double-bills ~0. Initialised at lease issue (metered_* = 0, last_metered_at
--   = issue time) so the first /renew meters off a never-NULL cursor; carried
--   forward on reclaim so the re-leased holder meters from where the dead one
--   stopped (no double-charge, no gap).

CREATE TABLE IF NOT EXISTS fleet.runner_leases (
    uid               UUID   GENERATED ALWAYS AS (id) STORED PRIMARY KEY,
    CONSTRAINT ck_runner_leases_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    id                UUID   NOT NULL UNIQUE,
    runner_id         UUID   NOT NULL REFERENCES fleet.runners(id) ON DELETE CASCADE,
    zombie_id         UUID   NOT NULL,
    workspace_id      UUID   NOT NULL,
    tenant_id         UUID   NOT NULL,
    event_id          TEXT   NOT NULL,
    actor             TEXT   NOT NULL,
    event_type        TEXT   NOT NULL,
    request_json      TEXT   NOT NULL,
    event_created_at  BIGINT NOT NULL,
    posture           TEXT   NOT NULL,
    provider          TEXT   NOT NULL,
    model             TEXT   NOT NULL,
    metered_input_tokens   BIGINT NOT NULL,
    metered_cached_tokens  BIGINT NOT NULL,
    metered_output_tokens  BIGINT NOT NULL,
    last_metered_at_ms     BIGINT NOT NULL,
    fencing_token     BIGINT NOT NULL,
    lease_expires_at  BIGINT NOT NULL,
    status            TEXT   NOT NULL,
    created_at        BIGINT NOT NULL,
    updated_at        BIGINT NOT NULL
);

-- api_runtime: the serve tier owns /v1/runners/me/{leases,reports}; it inserts
-- a lease at lease-issue and reads + updates status at report.
GRANT SELECT, INSERT, UPDATE ON fleet.runner_leases TO api_runtime;
