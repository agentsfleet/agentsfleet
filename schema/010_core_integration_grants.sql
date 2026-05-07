-- Integration grants for zombie-to-service authorization. A zombie must
-- have an approved grant for a service before usezombie will inject
-- credentials for it. Zombie-initiated, human-approved.

CREATE TABLE IF NOT EXISTS core.integration_grants (
    grant_id        TEXT    NOT NULL PRIMARY KEY,
    zombie_id       UUID    NOT NULL REFERENCES core.zombies(id) ON DELETE CASCADE,
    service         TEXT    NOT NULL,
    status          TEXT    NOT NULL DEFAULT 'pending',
    requested_at    BIGINT  NOT NULL,
    requested_reason TEXT   NOT NULL,
    approved_at     BIGINT  NULL,
    revoked_at      BIGINT  NULL,
    CONSTRAINT ck_integration_grants_status
        CHECK (status IN ('pending', 'approved', 'revoked')),
    CONSTRAINT ck_integration_grants_service
        CHECK (service IN ('slack', 'gmail', 'agentmail', 'discord', 'grafana')),
    CONSTRAINT uq_integration_grants_zombie_service
        UNIQUE (zombie_id, service)
);

CREATE INDEX IF NOT EXISTS idx_integration_grants_zombie_id
    ON core.integration_grants (zombie_id);

GRANT SELECT, INSERT, UPDATE ON core.integration_grants TO api_runtime;
GRANT SELECT ON core.integration_grants TO worker_runtime;
