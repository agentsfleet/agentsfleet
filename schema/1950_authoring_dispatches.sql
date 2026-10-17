-- One physical model call inside an attempt: the intent is written before the
-- call, the result after. A table, not attempt columns, because an attempt makes
-- many calls and a crash between intent and result must be visible.
--
-- `result` is private model output, scrubbed (set NULL) at `private_expires_at`
-- and at once on every erasure root (deletion matrix); the expiry index reads it.
-- `usage` is what settlement charges. Rows stay with their attempt.

CREATE TABLE authoring.dispatches (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_dispatches_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    attempt_id          UUID    NOT NULL REFERENCES authoring.attempts(id) ON DELETE CASCADE,
    sequence            INTEGER NOT NULL,
    request_digest      TEXT    NOT NULL,
    status              TEXT    NOT NULL,
    result              JSONB,
    usage               JSONB,
    private_expires_at  TIMESTAMPTZ(3)  NOT NULL,
    created_at          TIMESTAMPTZ(3)  NOT NULL,
    updated_at          TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_dispatches_attempt_id_sequence UNIQUE (attempt_id, sequence)
);

-- The scrub sweep over results still held.
CREATE INDEX idx_dispatches_private_expires_at_id
    ON authoring.dispatches (private_expires_at, id) WHERE result IS NOT NULL;

GRANT SELECT, INSERT, UPDATE ON authoring.dispatches TO api_runtime;
