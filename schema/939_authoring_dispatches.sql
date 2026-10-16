-- Commit intent before the physical provider call. An unfinished intent is
-- never sent again. Bounded result is private and is scrubbed on draft purge;
-- digest, pinned usage and dispatch identity are retained for accounting.
CREATE TABLE IF NOT EXISTS fleet.authoring_dispatches (
    id                    UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_dispatches_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    attempt_id            UUID NOT NULL REFERENCES fleet.authoring_attempts(id) ON DELETE CASCADE,
    call_index            INTEGER NOT NULL,
    request_digest        TEXT NOT NULL,
    status                TEXT NOT NULL,
    output_allowance      BIGINT NOT NULL,
    result                JSONB,
    provider_continuation JSONB,
    usage                 JSONB,
    private_expires_at    BIGINT NOT NULL,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL,
    CONSTRAINT uq_authoring_dispatches_index UNIQUE (attempt_id, call_index)
);

CREATE INDEX IF NOT EXISTS idx_authoring_dispatches_private_expiry
    ON fleet.authoring_dispatches(private_expires_at, id)
    WHERE result IS NOT NULL OR provider_continuation IS NOT NULL;
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.authoring_dispatches TO api_runtime;

-- Proposal receipts retain the originally committed candidate version.
-- No transcript or provider result is needed to acknowledge an exact replay.
CREATE TABLE IF NOT EXISTS fleet.authoring_activity_receipts (
    id                    UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_activity_receipts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    attempt_id            UUID NOT NULL REFERENCES fleet.authoring_attempts(id) ON DELETE CASCADE,
    activity_sequence     BIGINT NOT NULL,
    request_digest        TEXT NOT NULL,
    candidate_id          UUID,
    candidate_revision    BIGINT,
    created_at            BIGINT NOT NULL,
    CONSTRAINT uq_authoring_activity_receipts_sequence UNIQUE (attempt_id, activity_sequence)
);
GRANT SELECT, INSERT, DELETE ON fleet.authoring_activity_receipts TO api_runtime;
