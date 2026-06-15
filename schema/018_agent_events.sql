-- Per-event narrative log: one row per delivery into agent:{id}:events.
--
-- Mutable INSERT received → UPDATE terminal:
--   INSERT at start (status='received')                    — write path step 2
--   UPDATE at end   (status='processed' | 'agent_error')   — write path step 9
--   UPDATE on gate  (status='gate_blocked')                — write path step 4
--
-- Joined to agent_execution_telemetry by event_id (1:1, write-once telemetry row).
-- Joined to agent_sessions by agent_id (1:N, current session bookmark).
--
-- Idempotent on replay: UNIQUE (agent_id, event_id) + ON CONFLICT DO NOTHING.
-- The status enum and event_type enum are enforced in application code. No SQL CHECK
-- (CHECK with literal strings drifts silently from Zig/JS constants).

CREATE TABLE IF NOT EXISTS core.agent_events (
    uid              UUID    NOT NULL,
    agent_id        UUID    NOT NULL REFERENCES core.agents(id) ON DELETE CASCADE,
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
    CONSTRAINT ck_agent_events_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    CONSTRAINT uq_agent_events_agent_event UNIQUE (agent_id, event_id)
);

-- Per-agent history newest-first. Covers the dashboard's primary view
-- (no-actor listing) and keyset-cursor pagination via the (created_at,
-- event_id) tuple. Actor-filtered reads use this index too via
-- seek-by-agent + scan-and-filter; with LIMIT 50 and most-recent-first
-- ordering this satisfies the limit in a few pages even on chatty
-- agents. If actor filtering becomes a measured bottleneck a partial
-- or expression index can be added back.
CREATE INDEX IF NOT EXISTS agent_events_agent_idx
    ON core.agent_events (agent_id, created_at DESC, event_id DESC);

-- Workspace-aggregate history feeding the dashboard workspace overview.
CREATE INDEX IF NOT EXISTS agent_events_workspace_idx
    ON core.agent_events (workspace_id, created_at DESC);

-- Continuation chain walks (context-chunk continuations, gate-resolved re-enqueue).
-- Partial index — only continuation rows carry resumes_event_id.
CREATE INDEX IF NOT EXISTS agent_events_resumes_idx
    ON core.agent_events (agent_id, resumes_event_id)
    WHERE resumes_event_id IS NOT NULL;

-- api_runtime writes the lifecycle (INSERT received, UPDATE terminal) in the
-- lease/report path and serves the read endpoints (per-agent +
-- workspace-aggregate + SSE backfill).
GRANT SELECT, INSERT, UPDATE ON core.agent_events TO api_runtime;
