-- fleet.runner_events — append-only runner history for the operator plane.
--
-- `fleet.runners.admin_state` is operator intent and liveness is derived at
-- read time; this table is the durable history of runner lifecycle and work
-- transitions. Event type values are app-enforced enum tags (RULE STS: no SQL
-- CHECK with literal value sets). metadata is JSONB so event-specific details
-- can ride beside the typed event without bloating the current-state row.
--
-- dedup_key is reserved for the liveness sweeper: runner_offline uses the stale
-- last_seen_at snapshot so N replicas racing the same stale runner admit only
-- one offline event.

CREATE TABLE IF NOT EXISTS fleet.runner_events (
    uid          UUID   GENERATED ALWAYS AS (id) STORED PRIMARY KEY,
    id           UUID   NOT NULL UNIQUE,
    CONSTRAINT ck_runner_events_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    runner_id    UUID   NOT NULL REFERENCES fleet.runners(id) ON DELETE CASCADE,
    event_type   TEXT   NOT NULL,
    occurred_at  BIGINT NOT NULL,
    metadata     JSONB  NOT NULL,
    dedup_key    BIGINT NULL,
    created_at   BIGINT NOT NULL
);

-- Per-runner history newest-first for GET /v1/fleets/runners/{id}/events.
CREATE INDEX IF NOT EXISTS runner_events_runner_idx
    ON fleet.runner_events (runner_id, occurred_at DESC, id DESC);

-- Mechanism X: exactly one runner_offline audit row per stale last_seen_at
-- episode across all agentsfleetd replicas. The static value is the enum tag used by
-- protocol.RunnerEventType.runner_offline; partial indexes need a SQL predicate.
CREATE UNIQUE INDEX IF NOT EXISTS runner_events_offline_dedup_idx
    ON fleet.runner_events (runner_id, dedup_key)
    WHERE event_type = 'runner_offline' AND dedup_key IS NOT NULL;

-- api_runtime appends lifecycle events and serves the operator history endpoint.
-- No UPDATE/DELETE grant: append-only by privilege.
GRANT SELECT, INSERT ON fleet.runner_events TO api_runtime;
