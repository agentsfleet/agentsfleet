-- M2_001: Zombie credential storage (names + encrypted values per workspace).
-- Credential values are stored encrypted; API responses return names only.

CREATE TABLE IF NOT EXISTS core.zombie_credentials (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    name TEXT NOT NULL,
    value_encrypted TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT ck_zombie_credentials_id_uuidv7
        CHECK (substring(id::text from 15 for 1) = '7'),
    CONSTRAINT uq_zombie_credentials_workspace_name
        UNIQUE (workspace_id, name)
);
