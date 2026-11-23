-- Reachable only through role `memory_runtime`, which api_runtime must SET ROLE into.
-- `workspace_id` copies the fleet's workspace, written once with the entry, because
-- memory_runtime has no grant on `fleet.fleets`.
CREATE TABLE memory.entries (
    id                 UUID PRIMARY KEY,
    fleet_id           UUID    NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    workspace_id       UUID    NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    key                TEXT    NOT NULL,
    content            TEXT    NOT NULL,
    category           TEXT    NOT NULL,
    workspace_visible  BOOLEAN NOT NULL,
    created_at         TIMESTAMPTZ(3)  NOT NULL,
    updated_at         TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_entries_fleet_id_key UNIQUE (fleet_id, key)
);

CREATE INDEX idx_entries_fleet_id_category_updated_at
    ON memory.entries (fleet_id, category, updated_at);

CREATE INDEX idx_entries_fleet_id_created_at_key
    ON memory.entries (fleet_id, created_at DESC, key DESC);

CREATE INDEX idx_entries_fleet_id_updated_at_id
    ON memory.entries (fleet_id, updated_at DESC, id DESC);

CREATE INDEX idx_entries_workspace_id_shared_updated_at
    ON memory.entries (workspace_id, updated_at DESC, id DESC) WHERE workspace_visible;

GRANT SELECT, INSERT, UPDATE, DELETE ON memory.entries TO memory_runtime;
