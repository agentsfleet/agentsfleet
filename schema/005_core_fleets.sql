-- Fleet runtime table.
-- source_markdown: raw SKILL.md (Fleet instructions)
-- trigger_markdown: raw TRIGGER.md (deployment manifest)
-- config_json: server-computed from trigger_markdown frontmatter
-- Webhook Hash-based Message Authentication Code (HMAC) secrets live in vault.secrets keyed by `fleet:<source>` (or
-- `fleet:<credential_name>` when the trigger frontmatter overrides) -- this
-- table holds no secret pointers.
-- Status transitions: active → paused → active | active → stopped (terminal)
-- Status values enforced in application code (fleet status constants).
-- required_tags: capability tags this Fleet needs to be placed (GitLab-tags /
--   GitHub-labels model). A runner may claim it only when required_tags is a
--   subset of the runner's fleet.runners.labels (fleet.assign.listCandidates).
--   Empty set = any runner (today's behaviour). App-supplied, bounds-validated
--   on create/config (≤32 tags, 1..64 chars each → UZ-REQ-001). Not deduplicated:
--   `<@` containment is set-semantic, so duplicate entries are harmless.
--   Stored as TEXT[] (not JSONB): a string-set needs no nesting, and only the
--   array `array_ops` GIN opclass supports `<@`, so the eligibility filter is
--   index-eligible when the runner's labels are bound as a constant array.
-- bundle_content_hash / bundle_snapshot_key: the content identity of the
--   onboarded template a Fleet was installed from. The runner materializes the
--   support files from R2 by content hash. No secret values live here;
--   credentials remain vault refs.

CREATE TABLE IF NOT EXISTS core.fleets (
    uid             UUID GENERATED ALWAYS AS (id) STORED PRIMARY KEY,
    CONSTRAINT ck_fleets_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    id              UUID NOT NULL UNIQUE,
    workspace_id    UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    name            TEXT NOT NULL,
    source_markdown TEXT NOT NULL,
    trigger_markdown TEXT,
    config_json     JSONB NOT NULL,
    status          TEXT NOT NULL,
    -- The empty array is the only valid initial value (the any-runner identity),
-- so it carries a structural DEFAULT -- same exception class as
-- fleet.runner_affinity.meter_slice_seq's DEFAULT 0, not a Static Strings in SQL (STS) enum-value
    -- default that mirrors a code constant. The create path always writes the
    -- validated set explicitly; the default keeps unrelated inserts from
    -- re-stating it.
    required_tags   TEXT[] NOT NULL DEFAULT '{}'::text[],
    bundle_content_hash TEXT,
    bundle_snapshot_key TEXT,
    -- Denormalized activity counters, maintained at write time by the triggers
    -- in migration 028. They mirror COUNT(core.fleet_events) and
    -- SUM(core.fleet_execution_telemetry.credit_deducted_nanos) for this fleet,
    -- so the fleets list / detail (the Live Wall hot path) read them as plain
    -- columns instead of re-aggregating the child tables on every read. Same
    -- structural DEFAULT-0 class as required_tags above -- a counter's zero
    -- origin, not an STS enum-value default mirroring a code constant.
    events_processed  BIGINT NOT NULL DEFAULT 0,
    budget_used_nanos BIGINT NOT NULL DEFAULT 0,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,
    CONSTRAINT uq_fleets_workspace_id_name UNIQUE (workspace_id, name)
);

-- api_runtime creates, reads, updates Fleets for Command-Line Interface (CLI) install/up/kill operations
-- and reads config + status at lease-issue time.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleets TO api_runtime;

-- Partial index for Slack event routing: find the active Fleet with a
-- slack_event trigger for a given workspace (lookupSlackFleet in slack_events.zig).
-- Partial on status='active' keeps the index small; workspace_id+created_at
-- covers the equality filter and deterministic ORDER BY in one scan.
CREATE INDEX IF NOT EXISTS idx_fleets_workspace_id_created_at_active
    ON core.fleets(workspace_id, created_at)
    WHERE status = 'active';

-- Generalized Inverted Index (GIN) for the runner-placement eligibility filter (required_tags <@ labels
-- in fleet.assign.listCandidates). array_ops supports <@, so the candidate scan
-- can prune by tag once the polling runner's labels are bound as a constant
-- array. (Confirm planner usage with EXPLAIN once the feature carries real data —
-- <@ is GIN's weak direction and the empty-set majority is unselective.)
CREATE INDEX IF NOT EXISTS idx_fleets_required_tags_gin
    ON core.fleets USING gin (required_tags);
