-- Singleton: the catalogue's compare-and-swap edit counter, read with every price
-- so a cached price list knows when it is stale. One row, pinned to `id = 1` (an
-- identity exception: there is nothing to mint a UUID for).
--
-- The seed row is structural, not vocabulary: id 1, counter 0, stamped when the slot runs.

CREATE TABLE model.catalogue_version (
    id                SMALLINT PRIMARY KEY,
    resource_version  BIGINT NOT NULL,
    created_at        TIMESTAMPTZ(3) NOT NULL,
    updated_at        TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_catalogue_version_singleton CHECK (id = 1)
);

INSERT INTO model.catalogue_version (id, resource_version, created_at, updated_at)
VALUES (1, 0, now(), now());

GRANT SELECT, UPDATE ON model.catalogue_version TO api_runtime;
