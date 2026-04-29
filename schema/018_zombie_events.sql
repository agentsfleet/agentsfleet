-- Per-event narrative log: one row per delivery into zombie:{id}:events.
--
-- Mutable INSERT received → UPDATE terminal:
--   INSERT at start (status='received')                    — write path step 2
--   UPDATE at end   (status='processed' | 'agent_error')   — write path step 9
--   UPDATE on gate  (status='gate_blocked')                — write path step 4
--
-- Joined to zombie_execution_telemetry by event_id (1:1, write-once telemetry row).
-- Joined to zombie_sessions by zombie_id (1:N, current session bookmark).
--
-- Idempotent on replay: PRIMARY KEY (zombie_id, event_id) + ON CONFLICT DO NOTHING.
-- The status enum and event_type enum are enforced in application code. No SQL CHECK
-- (CHECK with literal strings drifts silently from Zig/JS constants).

CREATE TABLE IF NOT EXISTS core.zombie_events (
    zombie_id        UUID    NOT NULL REFERENCES core.zombies(id) ON DELETE CASCADE,
    event_id         TEXT    NOT NULL,
    workspace_id     UUID    NOT NULL,
    actor            TEXT    NOT NULL,
    event_type       TEXT    NOT NULL,
    status           TEXT    NOT NULL,
    request_json     JSONB   NOT NULL,
    response_text    TEXT    NULL,
    tokens           BIGINT  NULL,
    wall_ms          BIGINT  NULL,
    failure_label    TEXT    NULL,
    checkpoint_id    TEXT    NULL,
    resumes_event_id TEXT    NULL,
    created_at       BIGINT  NOT NULL,
    updated_at       BIGINT  NOT NULL,
    PRIMARY KEY (zombie_id, event_id)
);

-- Per-zombie history with actor filter (e.g. all webhook deliveries newest-first).
CREATE INDEX IF NOT EXISTS zombie_events_actor_idx
    ON core.zombie_events (zombie_id, actor, created_at DESC);

-- Workspace-aggregate history feeding the dashboard workspace overview.
CREATE INDEX IF NOT EXISTS zombie_events_workspace_idx
    ON core.zombie_events (workspace_id, created_at DESC);

-- Continuation chain walks (context-chunk continuations, gate-resolved re-enqueue).
-- Partial index — only continuation rows carry resumes_event_id.
CREATE INDEX IF NOT EXISTS zombie_events_resumes_idx
    ON core.zombie_events (zombie_id, resumes_event_id)
    WHERE resumes_event_id IS NOT NULL;

-- worker_runtime writes the lifecycle (INSERT received, UPDATE terminal).
GRANT SELECT, INSERT, UPDATE ON core.zombie_events TO worker_runtime;
-- api_runtime serves the read endpoints (per-zombie + workspace-aggregate + SSE backfill).
GRANT SELECT ON core.zombie_events TO api_runtime;
