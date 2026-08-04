-- Platform Fleet Library catalogue (curated, global).
--
-- Runtime-owned: this table carries NO seed. A row is born when a platform
-- operator holding `platform-library:write` adds a repository from
-- /admin/fleet-libraries (POST /v1/admin/fleet-libraries), which fetches the
-- bundle, validates it, writes the canonical tar to object storage, and derives
-- id/name/description/credentials/tools/hosts from the bundle's SKILL.md
-- frontmatter. Nothing in this directory inserts a catalogue row, and an INSERT
-- here would be a bug.
--
-- Identity exception (SCHEMA_CONVENTIONS "Identity Column"): a curated catalogue
-- keyed by a stable slug carries no UUID. `id` is the bundle's frontmatter name
-- and the stable public identifier — a reference catalogue, not a per-tenant
-- entity, so a minted surrogate would add a second key nothing addresses.
--
-- `visibility` is the publish lifecycle, not a tier: tenant entries live in
-- `core.tenant_fleet_library`, so a row here is only ever 'draft' (bundle
-- stored, invisible to tenants) or 'public' (live in every workspace gallery).
-- Every write stages to 'draft'; publishing is an explicit, reversible PATCH.
-- Three readers gate on it — the workspace gallery, GET /v1/fleets/bundles, and
-- the resolve-by-id install path — so an unpublished fleet is unreachable rather
-- than merely unlisted. Canonical constants: fleet_library/library_store.zig
-- (VISIBILITY_DRAFT / VISIBILITY_PUBLIC). Value sets are app-enforced per RULE
-- STS, and no statement in this directory writes them.
--
-- A row that is 'public' but holds NO bundle cannot lie to a tenant: all three
-- tenant-facing reads filter on `content_hash IS NOT NULL` as well as on
-- visibility. That is enforced in the queries, not by migrating rows.
--
-- Layout decision (eng-review 2026-06-20, FINAL): ONE GIT REPOSITORY PER ENTRY,
-- named agentsfleet/<id> (repository name == entry id). The repository ROOT is
-- the bundle (SKILL.md at root, optional TRIGGER.md, support files including
-- subfolders), so `source_path` is empty and the importer strips the single
-- tarball wrapper directory — no subpath filter. Fetch is a cold path
-- (import-time, object-storage-cached by content hash afterwards).

CREATE TABLE IF NOT EXISTS core.fleet_library (
    id                   TEXT  PRIMARY KEY,
    name                 TEXT  NOT NULL,
    description          TEXT  NOT NULL,
    source_repo          TEXT  NOT NULL,
    source_path          TEXT  NOT NULL,
    source_ref           TEXT  NOT NULL,
    required_credentials JSONB NOT NULL,
    -- Per-credential "why this fleet needs it" copy, keyed by credential name
    -- (for example {"github":"review your pull requests"}). Operator-owned: the
    -- importer cannot derive it, so a new row starts with {} and an operator
    -- writes it via PATCH. A refetch must never clobber it, so it is absent from
    -- the upsert's ON CONFLICT list on purpose. Display-only preview copy the
    -- install gate renders so the user knows why to connect — NOT a security
    -- control; credential validation reads `required_credentials`.
    required_credentials_reasons JSONB NOT NULL,
    required_tools       JSONB NOT NULL,
    network_hosts        JSONB NOT NULL,
    visibility           TEXT  NOT NULL,
    -- Bundle snapshot, filled by the add/refetch write. `content_hash` points at
    -- the stored tar (fleet-bundles/sha256/{hash}.tar); `support_files_json` is
    -- a path/size/hash manifest carrying no body content. Nullable because a row
    -- can outlive its bundle in one direction only — an unpublished row may not
    -- have one yet, and a published row always does.
    content_hash         TEXT,
    skill_markdown       TEXT,
    trigger_markdown     TEXT,
    support_files_json   JSONB,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL
);

-- No index. The catalogue is operator-curated and small; the gallery and bundle
-- list read it whole, and every targeted read is by `id`, which is the primary
-- key. A `visibility` index would be a btree over two values with no selectivity
-- to offer.

-- api_runtime serves the catalogue (GET /v1/fleets/bundles, the workspace
-- gallery, GET /v1/admin/fleet-libraries) and owns its whole lifecycle:
-- add/refetch (INSERT/UPDATE), curate and publish/unpublish (UPDATE), and delete
-- an unpublished row (DELETE). Every write is gated in-handler by the
-- platform-library:write scope (requireScope middleware).
GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleet_library TO api_runtime;
