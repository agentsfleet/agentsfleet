-- Small dashboard settings per (user, workspace), read by the dashboard on load.
-- A table, not a users column, because a setting is scoped to a workspace too.

CREATE TABLE identity.user_preferences (
    id            UUID PRIMARY KEY,
    CONSTRAINT ck_user_preferences_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    user_id       UUID NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    workspace_id  UUID NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    value         TEXT NOT NULL,
    created_at    TIMESTAMPTZ(3) NOT NULL,
    updated_at    TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_user_preferences_user_id_workspace_id_name
        UNIQUE (user_id, workspace_id, name)
);

CREATE INDEX idx_user_preferences_workspace_id ON identity.user_preferences (workspace_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.user_preferences TO api_runtime;
