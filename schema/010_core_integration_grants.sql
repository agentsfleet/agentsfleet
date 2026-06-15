-- Integration grants for agent-to-service authorization. A agent must
-- have an approved grant for a service before usezombie will inject
-- credentials for it. Agent-initiated, human-approved.

CREATE TABLE IF NOT EXISTS core.integration_grants (
    uid             UUID    PRIMARY KEY,
    grant_id        TEXT    NOT NULL UNIQUE,
    agent_id       UUID    NOT NULL REFERENCES core.agents(id) ON DELETE CASCADE,
    service         TEXT    NOT NULL,
    status          TEXT    NOT NULL,
    requested_at    BIGINT  NOT NULL,
    requested_reason TEXT   NOT NULL,
    approved_at     BIGINT  NULL,
    revoked_at      BIGINT  NULL,
    CONSTRAINT ck_integration_grants_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    CONSTRAINT ck_integration_grants_grant_id_uuidv7
        CHECK (grant_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'),
    CONSTRAINT ck_integration_grants_uid_matches_grant_id CHECK (uid::text = grant_id),
    CONSTRAINT uq_integration_grants_agent_service
        UNIQUE (agent_id, service)
);

CREATE INDEX IF NOT EXISTS idx_integration_grants_agent_id
    ON core.integration_grants (agent_id);

GRANT SELECT, INSERT, UPDATE ON core.integration_grants TO api_runtime;
