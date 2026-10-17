-- Singleton: the catalogue's compare-and-swap edit counter, read with every price
-- so a cached price list knows when it is stale. One row, pinned to `id = 1` (an
-- identity exception: there is nothing to mint a UUID for).
--
-- The seed row is structural, not vocabulary: id 1 and counter 0.

CREATE TABLE model.catalogue_version (
    id                SMALLINT PRIMARY KEY,
    resource_version  BIGINT NOT NULL,
    created_at        BIGINT NOT NULL,
    updated_at        BIGINT NOT NULL,
    CONSTRAINT ck_catalogue_version_singleton CHECK (id = 1)
);

INSERT INTO model.catalogue_version (id, resource_version, created_at, updated_at)
VALUES (1, 0, 0, 0);

GRANT SELECT, UPDATE ON model.catalogue_version TO api_runtime;
