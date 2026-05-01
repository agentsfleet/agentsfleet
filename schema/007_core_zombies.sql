-- Zombie entity table.
-- source_markdown: raw SKILL.md (agent instructions)
-- trigger_markdown: raw TRIGGER.md (deployment manifest)
-- config_json: server-computed from trigger_markdown frontmatter
-- Webhook HMAC secrets live in vault.secrets keyed by `zombie:<source>` (or
-- `zombie:<credential_name>` when the trigger frontmatter overrides) — this
-- table holds no secret pointers.
-- Status transitions: active → paused → active | active → stopped (terminal)
-- Status values enforced in application code (error_codes.ZOMBIE_STATUS_*)

CREATE TABLE IF NOT EXISTS core.zombies (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_zombies_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id    UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    name            TEXT NOT NULL,
    source_markdown TEXT NOT NULL,
    trigger_markdown TEXT,
    config_json     JSONB NOT NULL,
    status          TEXT NOT NULL,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,
    CONSTRAINT uq_zombies_workspace_name UNIQUE (workspace_id, name)
);

-- Worker reads config and status at claim time.
-- API creates, reads, updates zombies for CLI install/up/kill operations.
GRANT SELECT ON core.zombies TO worker_runtime;
GRANT SELECT, INSERT, UPDATE ON core.zombies TO api_runtime;

-- Partial index for Slack event routing: find the active zombie with a
-- slack_event trigger for a given workspace (lookupSlackZombie in slack_events.zig).
-- Partial on status='active' keeps the index small; workspace_id+created_at
-- covers the equality filter and deterministic ORDER BY in one scan.
CREATE INDEX IF NOT EXISTS idx_zombies_slack_event_trigger
    ON core.zombies(workspace_id, created_at)
    WHERE status = 'active';
