-- M1_001 §6.1: Zombie entity table.
-- source_markdown: raw .md file (YAML frontmatter + freeform instructions/voice transcript).
-- config_json: CLI-parsed frontmatter as JSON. Zig server reads this only, never parses YAML.
-- Status transitions: active → paused → active | active → stopped (soft) | active → killed (terminal).
-- Status validation is enforced by ZombieStatus enum in src/zombie/config.zig, not by CHECK constraint.

CREATE TABLE IF NOT EXISTS core.zombies (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_zombies_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id    UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    name            TEXT NOT NULL,
    source_markdown TEXT NOT NULL,
    config_json     JSONB NOT NULL,
    status          TEXT NOT NULL DEFAULT 'active',
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,
    CONSTRAINT uq_zombies_workspace_name UNIQUE (workspace_id, name)
);

-- Worker reads config and status at claim time.
-- API creates, reads, updates zombies for CLI install/up/kill operations.
GRANT SELECT ON core.zombies TO worker_runtime;
GRANT SELECT, INSERT, UPDATE ON core.zombies TO api_runtime;
