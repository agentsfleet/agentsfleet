-- Platform Fleet Library catalog (curated, global).
--
-- Runtime-owned (M128): this table carries NO seed. A row is born when a
-- platform operator holding platform-library:write adds a repository from
-- /admin/fleet-libraries (POST /v1/admin/fleet-libraries), which fetches the
-- bundle, validates it, writes the canonical tar to object storage, and derives
-- id/name/description/credentials/tools/hosts from the bundle's SKILL.md
-- frontmatter. Nothing here inserts a catalog row — an INSERT in this directory
-- is a bug (M128 Invariant 5).
--
-- The `visibility` column is the publish lifecycle, not a tier: tenant entries
-- live in core.tenant_fleet_library, so this table's rows are only ever
-- 'draft' (bundle stored, invisible to tenants) or 'public' (live in every
-- workspace gallery). Every write stages to 'draft'; publishing is an explicit,
-- reversible PATCH. Three readers gate on it — the workspace gallery,
-- GET /v1/fleets/bundles, and the resolve-by-id install path — so an
-- unpublished fleet is unreachable, not merely unlisted.
-- Canonical constants: fleet_library/library_store.zig (VISIBILITY_DRAFT /
-- VISIBILITY_PUBLIC). Value sets are enforced in application code per RULE STS
-- (no static-string DEFAULT/CHECK here).
--
-- Layout decision (eng-review 2026-06-20, FINAL): ONE GIT REPO PER ENTRY, named
-- agentsfleet/<id> (repo name == entry id). The repo ROOT is the bundle
-- (SKILL.md at root, optional TRIGGER.md, support files incl. subfolders), so
-- source_path is empty and the importer just strips the single tarball wrapper
-- dir — no subpath filter. Fetch is a cold path (import-time, R2-cached by
-- content hash after).
--
-- Keyed by slug (the stable API id == the bundle's frontmatter name) rather than
-- a UUIDv7: this is a curated reference catalog, not a per-tenant entity.
CREATE TABLE IF NOT EXISTS core.fleet_library (
    id                   TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    description          TEXT NOT NULL,
    source_repo          TEXT NOT NULL,
    source_path          TEXT NOT NULL,
    source_ref           TEXT NOT NULL,
    required_credentials JSONB NOT NULL,
    -- Per-credential "why this fleet needs it" copy, keyed by credential name
    -- (e.g. {"github":"review your pull requests"}). Operator-owned: the importer
    -- cannot derive it, so a new row starts with {} and an operator writes it via
    -- PATCH. A refetch must never clobber it — it is absent from the upsert's
    -- ON CONFLICT list on purpose. Display-only preview copy the install gate
    -- renders so the user knows why to connect — NOT a security control;
    -- credential validation reads required_credentials.
    required_credentials_reasons JSONB NOT NULL,
    required_tools       JSONB NOT NULL,
    network_hosts        JSONB NOT NULL,
    -- Publish lifecycle: 'draft' | 'public'. See the header.
    visibility           TEXT NOT NULL,
    -- Bundle snapshot: filled by the add/refetch write. content_hash points to
    -- the R2 tar (fleet-bundles/sha256/{hash}.tar); support_files_json stores a
    -- path/size/hash manifest (no body content). Nullable because a row can
    -- outlive its bundle only in one direction — a deployed database may still
    -- hold pre-M128 rows that never had one, and those can never be published
    -- (a published row always has a bundle; M128 Invariant 1).
    content_hash         TEXT,
    skill_markdown       TEXT,
    trigger_markdown     TEXT,
    support_files_json   JSONB,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL
);

-- api_runtime serves the catalog (GET /v1/fleets/bundles, the workspace gallery,
-- GET /v1/admin/fleet-libraries) and owns its whole lifecycle: add/refetch
-- (INSERT/UPDATE), curate + publish/unpublish (UPDATE), and delete an
-- unpublished row (DELETE). Every write is gated in-handler by the
-- platform-library:write scope (requireScope middleware).
GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleet_library TO api_runtime;
