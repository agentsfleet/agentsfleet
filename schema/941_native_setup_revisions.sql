-- Library publication and fleet activation copy native, immutable setup.
-- Private candidates can then be deleted without affecting either shared copy.
CREATE TABLE IF NOT EXISTS core.fleet_libraries (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_fleet_libraries_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id UUID REFERENCES core.tenants(id) ON DELETE CASCADE,
    visibility TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    head_revision_id UUID,
    resource_version BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_fleet_libraries_tenant_page
    ON core.fleet_libraries(tenant_id, created_at DESC, id DESC);
GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleet_libraries TO api_runtime;

CREATE TABLE IF NOT EXISTS core.fleet_library_revisions (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_fleet_library_revisions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    -- No entry FK: deleting an entry hides its history while installed fleets
    -- retain their pinned snapshots. Unreferenced snapshots are swept later.
    library_id UUID NOT NULL,
    revision BIGINT NOT NULL,
    definition JSONB NOT NULL,
    canonical_digest TEXT NOT NULL,
    policy_semantics TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    CONSTRAINT uq_fleet_library_revisions_number UNIQUE (library_id, revision)
);
GRANT SELECT, INSERT, DELETE ON core.fleet_library_revisions TO api_runtime;

CREATE TABLE IF NOT EXISTS core.fleet_definition_revisions (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_fleet_definition_revisions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id UUID NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    revision BIGINT NOT NULL,
    definition JSONB NOT NULL,
    canonical_digest TEXT NOT NULL,
    policy_semantics TEXT NOT NULL,
    library_revision_id UUID REFERENCES core.fleet_library_revisions(id),
    created_at BIGINT NOT NULL,
    CONSTRAINT uq_fleet_definition_revisions_number UNIQUE (fleet_id, revision)
);
CREATE INDEX IF NOT EXISTS idx_fleet_definition_revisions_library
    ON core.fleet_definition_revisions(library_revision_id)
    WHERE library_revision_id IS NOT NULL;
GRANT SELECT, INSERT, DELETE ON core.fleet_definition_revisions TO api_runtime;

ALTER TABLE core.fleets ADD COLUMN IF NOT EXISTS current_definition_revision_id UUID;
ALTER TABLE core.fleets ADD COLUMN IF NOT EXISTS setup_version BIGINT;
