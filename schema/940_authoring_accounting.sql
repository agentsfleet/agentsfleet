-- Accounting identity excludes optional private answer content. The same
-- authenticated attempt/report can settle after deletion without restoring
-- text. A second report identity cannot settle an already settled attempt.
CREATE TABLE IF NOT EXISTS billing.authoring_settlements (
    id                    UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_settlements_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    attempt_id            UUID NOT NULL UNIQUE REFERENCES fleet.authoring_attempts(id) ON DELETE CASCADE,
    report_id             UUID NOT NULL,
    accounting_digest     TEXT NOT NULL,
    outcome               TEXT NOT NULL,
    failure_reason        TEXT,
    usage                 JSONB NOT NULL,
    charged_nanos         BIGINT NOT NULL,
    answer_digest         TEXT,
    created_at            BIGINT NOT NULL
);

-- Immutable accounting survives private text removal. Tenant/workspace
-- erasure cascades through the attempt; there is no direct UPDATE grant.
GRANT SELECT, INSERT ON billing.authoring_settlements TO api_runtime;
