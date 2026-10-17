-- A Library card: the gallery entry a fleet is installed from. NULL `tenant_id` is
-- a platform card, addressed by `platform_slug`; a tenant card is visible to that
-- tenant only. The card's content lives on its frozen revisions; `head_revision_id`
-- names the live one (its foreign key closes a cycle, so it lands in slot 1420).
--
-- `credential_reasons` is read by the install review, which explains why each
-- credential is asked for. `resource_version` is the compare-and-swap counter a
-- publish bumps. Deleting a card deletes its revisions (deletion matrix: Library
-- card); fleets installed from it keep their own frozen copy.

CREATE TABLE authoring.library_entries (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_library_entries_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id           UUID REFERENCES identity.tenants(id) ON DELETE CASCADE,
    visibility          TEXT   NOT NULL,
    platform_slug       TEXT,
    name                TEXT   NOT NULL,
    description         TEXT   NOT NULL,
    credential_reasons  JSONB,
    head_revision_id    UUID,
    resource_version    BIGINT NOT NULL,
    created_at          BIGINT NOT NULL,
    updated_at          BIGINT NOT NULL
);

-- The Library page, newest first, keyset-paged on (created_at, id).
CREATE INDEX idx_library_entries_tenant_id_created_at_id
    ON authoring.library_entries (tenant_id, created_at DESC, id DESC);

CREATE UNIQUE INDEX uq_library_entries_platform_slug
    ON authoring.library_entries (platform_slug) WHERE platform_slug IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON authoring.library_entries TO api_runtime;
