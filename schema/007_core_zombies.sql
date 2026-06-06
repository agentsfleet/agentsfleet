-- Zombie entity table.
-- source_markdown: raw SKILL.md (agent instructions)
-- trigger_markdown: raw TRIGGER.md (deployment manifest)
-- config_json: server-computed from trigger_markdown frontmatter
-- Webhook HMAC secrets live in vault.secrets keyed by `zombie:<source>` (or
-- `zombie:<credential_name>` when the trigger frontmatter overrides) — this
-- table holds no secret pointers.
-- Status transitions: active → paused → active | active → stopped (terminal)
-- Status values enforced in application code (error_codes.ZOMBIE_STATUS_*)
-- required_tags: capability tags this zombie needs to be placed (GitLab-tags /
--   GitHub-labels model). A runner may claim it only when required_tags is a
--   subset of the runner's fleet.runners.labels (fleet.assign.listCandidates).
--   Empty set = any runner (today's behaviour). App-supplied, bounds-validated
--   on create/config (≤32 tags, 1..64 chars each → UZ-REQ-001). Not deduplicated:
--   `<@` containment is set-semantic, so duplicate entries are harmless.
--   Stored as TEXT[] (not JSONB): a string-set needs no nesting, and only the
--   array `array_ops` GIN opclass supports `<@`, so the eligibility filter is
--   index-eligible when the runner's labels are bound as a constant array.

CREATE TABLE IF NOT EXISTS core.zombies (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_zombies_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id    UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    name            TEXT NOT NULL,
    source_markdown TEXT NOT NULL,
    trigger_markdown TEXT,
    config_json     JSONB NOT NULL,
    status          TEXT NOT NULL,
    -- The empty array is the only valid initial value (the any-runner identity),
    -- so it carries a structural DEFAULT — same exception class as
    -- fleet.runner_affinity.meter_slice_seq's DEFAULT 0, NOT an STS enum-value
    -- default that mirrors a code constant. The create path always writes the
    -- validated set explicitly; the default keeps unrelated inserts from
    -- re-stating it.
    required_tags   TEXT[] NOT NULL DEFAULT '{}'::text[],
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,
    CONSTRAINT uq_zombies_workspace_name UNIQUE (workspace_id, name)
);

-- api_runtime creates, reads, updates zombies for CLI install/up/kill operations
-- and reads config + status at lease-issue time.
GRANT SELECT, INSERT, UPDATE ON core.zombies TO api_runtime;

-- Partial index for Slack event routing: find the active zombie with a
-- slack_event trigger for a given workspace (lookupSlackZombie in slack_events.zig).
-- Partial on status='active' keeps the index small; workspace_id+created_at
-- covers the equality filter and deterministic ORDER BY in one scan.
CREATE INDEX IF NOT EXISTS idx_zombies_slack_event_trigger
    ON core.zombies(workspace_id, created_at)
    WHERE status = 'active';

-- GIN index for the runner-placement eligibility filter (required_tags <@ labels
-- in fleet.assign.listCandidates). array_ops supports <@, so the candidate scan
-- can prune by tag once the polling runner's labels are bound as a constant
-- array. (Confirm planner usage with EXPLAIN once the feature carries real data —
-- <@ is GIN's weak direction and the empty-set majority is unselective.)
CREATE INDEX IF NOT EXISTS idx_zombies_required_tags_gin
    ON core.zombies USING gin (required_tags);
