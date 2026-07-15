-- Per-user, per-workspace dashboard UI preferences: one row per named pref key.
--
-- The value is opaque to the server — it stores whatever small JSON the client
-- wrote and never interprets it beyond the key allowlist and a byte cap, both
-- enforced in the application (no CHECK constraint here: SQL cannot reference
-- the Zig/TypeScript key registry, so a schema-side list would drift).
--
-- Scope is (user, workspace) rather than (user) alone: onboarding progress is a
-- property of a workspace, so a second workspace starts its checklist fresh.

CREATE TABLE IF NOT EXISTS core.user_preferences (
    uid           UUID GENERATED ALWAYS AS (id) STORED PRIMARY KEY,
    CONSTRAINT ck_user_preferences_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    id            UUID NOT NULL UNIQUE,
    user_id       UUID NOT NULL REFERENCES core.users(user_id) ON DELETE CASCADE,
    workspace_id  UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    pref_key      TEXT NOT NULL,
    pref_value    TEXT NOT NULL,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL,
    CONSTRAINT uq_user_preferences_key UNIQUE (user_id, workspace_id, pref_key)
);

-- api_runtime backs GET/PUT /v1/workspaces/{workspace_id}/preferences. The unique
-- constraint above already indexes the (user_id, workspace_id) prefix the bag
-- read scans on, so no additional index is created.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.user_preferences TO api_runtime;
