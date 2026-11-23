-- Singleton edit counter for the model catalogue, read with every price so a cached price list
-- knows when it is stale. The seed row is structural.
CREATE TABLE model.revision (
    id          SMALLINT PRIMARY KEY,
    number      BIGINT NOT NULL,
    created_at  TIMESTAMPTZ(3) NOT NULL,
    updated_at  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_revision_singleton CHECK (id = 1)
);

INSERT INTO model.revision (id, number, created_at, updated_at)
VALUES (1, 0, now(), now());

GRANT SELECT, UPDATE ON model.revision TO api_runtime;
