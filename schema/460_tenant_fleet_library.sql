-- Tenant Fleet Library catalogue (workspace-scoped, runtime-onboarded).
--
-- The workspace-scoped counterpart to the global `core.fleet_library`. It
-- differs in two ways that matter. It is onboarded at runtime by a tenant admin
-- holding the `library:write` scope rather than curated by a platform operator,
-- which is why it carries a minted UUID identity while the platform catalogue is
-- keyed by a stable slug. And `support_files_json` stores a path/size/hash
-- manifest only, never support-file bodies; the object-storage key is derivable
-- from `content_hash` via importer.snapshotKey, so no key column is stored.
--
-- Value sets (`source_kind`, `visibility`) are enforced in application code —
-- SQL keeps them as TEXT per RULE STS.

CREATE TABLE IF NOT EXISTS core.tenant_fleet_library (
    id                  UUID   PRIMARY KEY,
    CONSTRAINT ck_tenant_fleet_library_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id        UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    name                TEXT   NOT NULL,
    -- SKILL.md description, surfaced on the gallery card. Mirrors
    -- `core.fleet_library.description`; the importer derives it from the
    -- onboarded SKILL frontmatter.
    description         TEXT   NOT NULL,
    source_kind         TEXT   NOT NULL,
    source_ref          TEXT   NOT NULL,
    visibility          TEXT   NOT NULL,
    content_hash        TEXT   NOT NULL,
    skill_markdown      TEXT   NOT NULL,
    trigger_markdown    TEXT,
    support_files_json  JSONB  NOT NULL,
    requirements_json   JSONB  NOT NULL,
    created_at          BIGINT NOT NULL,
    updated_at          BIGINT NOT NULL,
    -- Onboarding the same bundle twice into one workspace is one entry, so the
    -- domain key is (workspace_id, content_hash). It holds a different value
    -- from `id` rather than duplicating it, and it is what the onboarding
    -- upsert arbitrates.
    CONSTRAINT uq_tenant_fleet_library_workspace_id_content_hash
        UNIQUE (workspace_id, content_hash)
);

-- Reader: the workspace gallery, filtered by workspace and ordered newest-first.
-- The unique constraint above leads with `workspace_id` and so serves the filter
-- and the erasure cascade, but stops before `created_at` — this index carries
-- the sort, so the gallery is one seek rather than a seek plus a sort node.
CREATE INDEX IF NOT EXISTS idx_tenant_fleet_library_workspace_id_created_at
    ON core.tenant_fleet_library (workspace_id, created_at DESC);

-- api_runtime onboards (INSERT), reads (SELECT), and updates tenant entries.
-- No DELETE: an onboarded entry is retired by visibility, and the rows leave
-- with their workspace through the cascade above.
GRANT SELECT, INSERT, UPDATE ON core.tenant_fleet_library TO api_runtime;
