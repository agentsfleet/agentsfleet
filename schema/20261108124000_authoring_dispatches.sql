-- One physical model call. The intent is written before the call and the result after, so a
-- crash between them is visible. `result` and `native_reply` are private, never reach a runner,
-- and are scrubbed (set NULL) at `private_expires_at` or at once on erasure.
CREATE TABLE authoring.dispatches (
    id                  UUID PRIMARY KEY,
    attempt_id          UUID    NOT NULL REFERENCES authoring.attempts(id) ON DELETE CASCADE,
    sequence            INTEGER NOT NULL,
    request_digest      TEXT    NOT NULL,
    status              TEXT    NOT NULL,
    result              JSONB,
    usage               JSONB,
    native_reply        JSONB,
    private_expires_at  TIMESTAMPTZ(3)  NOT NULL,
    created_at          TIMESTAMPTZ(3)  NOT NULL,
    updated_at          TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_dispatches_attempt_id_sequence UNIQUE (attempt_id, sequence),
    -- `failure_cause` is content-free; it carries no model output.
    dispatch_expires_at TIMESTAMPTZ(3) NOT NULL,
    failure_cause TEXT
);

CREATE INDEX idx_dispatches_private_expires_at_id
    ON authoring.dispatches (private_expires_at, id)
    WHERE result IS NOT NULL OR native_reply IS NOT NULL;

GRANT SELECT, INSERT, UPDATE ON authoring.dispatches TO api_runtime;
