-- Per-user, per-workspace dashboard preferences: one row per named preference key.
--
-- The value is opaque to the server. It stores whatever small JSON the client
-- wrote and never interprets it beyond the key allowlist and a byte cap, both
-- enforced in the application — no CHECK here, because SQL cannot reference the
-- Zig and TypeScript key registry and a schema-side list would drift from it
-- (RULE STS).
--
-- Scope is (user, workspace) rather than user alone: onboarding progress is a
-- property of a workspace, so a second workspace starts its checklist fresh.

CREATE TABLE IF NOT EXISTS core.user_preferences (
    id            UUID   PRIMARY KEY,
    CONSTRAINT ck_user_preferences_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    user_id       UUID   NOT NULL REFERENCES core.users(id) ON DELETE CASCADE,
    workspace_id  UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    pref_key      TEXT   NOT NULL,
    pref_value    TEXT   NOT NULL,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL,
    -- One row per (user, workspace, key), and what the preference write
    -- arbitrates on ON CONFLICT.
    CONSTRAINT uq_user_preferences_user_id_workspace_id_pref_key
        UNIQUE (user_id, workspace_id, pref_key)
);

-- No index. The unique constraint above already indexes the
-- (user_id, workspace_id) prefix that the whole-bag read scans, and the
-- user cascade walks that same leading column.
--
-- The workspace cascade is deliberately left to a sequential scan: preferences
-- are a handful of rows per user per workspace, and deleting a workspace is a
-- once-in-an-account-lifetime operation, so an index maintained on every
-- preference write to speed it up would be the wrong trade.

-- api_runtime backs GET/PUT /v1/workspaces/{workspace_id}/preferences.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.user_preferences TO api_runtime;
