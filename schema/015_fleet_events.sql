-- Per-event narrative log: one row per delivery into fleet:{id}:events.
--
-- Mutable INSERT received → UPDATE terminal:
--   INSERT at start (status='received')                    — write path step 2
--   UPDATE at end   (status='processed' | 'fleet_error')   — write path step 9
--   UPDATE on gate  (status='gate_blocked')                — write path step 4
--
-- Joined to fleet_execution_telemetry by event_id (1:1, write-once telemetry row).
-- Joined to fleet_sessions by fleet_id (1:N, current session bookmark).
--
-- Idempotent on replay: UNIQUE (fleet_id, event_id) + ON CONFLICT DO NOTHING.
-- The status enum and event_type enum are enforced in application code. No SQL CHECK
-- (CHECK with literal strings drifts silently from Zig/JS constants).

CREATE TABLE IF NOT EXISTS core.fleet_events (
    uid              UUID    NOT NULL,
    fleet_id        UUID    NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    event_id         TEXT    NOT NULL,
    workspace_id     UUID    NOT NULL,
    actor            TEXT    NOT NULL,
    event_type       TEXT    NOT NULL,
    status           TEXT    NOT NULL,
    request_json     JSONB   NOT NULL,
    response_text    TEXT    NULL,
    tokens           BIGINT  NULL,
    wall_ms          BIGINT  NULL,
    -- Normalized failure cause (@tagName of contract.FailureClass); NULL on success.
    failure_label    TEXT    NULL,
    checkpoint_id    TEXT    NULL,
    resumes_event_id TEXT    NULL,
    created_at       BIGINT  NOT NULL,
    updated_at       BIGINT  NOT NULL,
    PRIMARY KEY (uid),
    CONSTRAINT ck_fleet_events_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    CONSTRAINT uq_fleet_events_fleet_id_event_id UNIQUE (fleet_id, event_id)
);

-- Per-fleet history newest-first. Covers the dashboard's primary view
-- (no-actor listing) and keyset-cursor pagination via the (created_at,
-- event_id) tuple. Actor-filtered reads use this index too via
-- seek-by-fleet + scan-and-filter; with LIMIT 50 and most-recent-first
-- ordering this satisfies the limit in a few pages even on chatty
-- fleets. If actor filtering becomes a measured bottleneck a partial
-- or expression index can be added back.
CREATE INDEX IF NOT EXISTS idx_fleet_events_fleet_id_created_at_event_id
    ON core.fleet_events (fleet_id, created_at DESC, event_id DESC);

-- Workspace-aggregate history feeding the dashboard workspace overview.
CREATE INDEX IF NOT EXISTS idx_fleet_events_workspace_id_created_at
    ON core.fleet_events (workspace_id, created_at DESC);

-- Continuation chain walks (context-chunk continuations, gate-resolved re-enqueue).
-- Partial index — only continuation rows carry resumes_event_id.
CREATE INDEX IF NOT EXISTS idx_fleet_events_fleet_id_resumes_event_id
    ON core.fleet_events (fleet_id, resumes_event_id)
    WHERE resumes_event_id IS NOT NULL;

-- api_runtime writes the lifecycle (INSERT received, UPDATE terminal) in the
-- lease/report path and serves the read endpoints (per-fleet +
-- workspace-aggregate + SSE backfill).
GRANT SELECT, INSERT, UPDATE ON core.fleet_events TO api_runtime;
