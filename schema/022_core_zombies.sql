-- M1_001 §6.1 + M2_002: Zombie entity table.
-- source_markdown: raw SKILL.md (agent instructions)
-- trigger_markdown: raw TRIGGER.md (deployment manifest)
-- config_json: server-computed from trigger_markdown frontmatter
-- webhook_secret_ref: vault key_name for the webhook secret (stored in vault.secrets)
-- Status transitions: active → paused → active | active → stopped (terminal)

CREATE TABLE IF NOT EXISTS core.zombies (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_zombies_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id    UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    name            TEXT NOT NULL,
    source_markdown TEXT NOT NULL,
    trigger_markdown TEXT,
    config_json     JSONB NOT NULL,
    webhook_secret_ref TEXT,
    status          TEXT NOT NULL DEFAULT 'active',
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,
    CONSTRAINT uq_zombies_workspace_name UNIQUE (workspace_id, name),
    CONSTRAINT ck_zombies_status CHECK (status IN ('active', 'paused', 'stopped'))
);

-- Worker reads config and status at claim time.
-- API creates, reads, updates zombies for CLI install/up/kill operations.
GRANT SELECT ON core.zombies TO worker_runtime;
GRANT SELECT, INSERT, UPDATE ON core.zombies TO api_runtime;
