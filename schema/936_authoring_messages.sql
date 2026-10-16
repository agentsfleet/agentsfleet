-- One request and its optional answer share a sequence. Acceptance pins the
-- selected model reference; changing the draft's selection cannot rewrite it.
CREATE TABLE IF NOT EXISTS core.authoring_messages (
    id                    UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_messages_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    draft_id              UUID NOT NULL REFERENCES core.authoring_drafts(id) ON DELETE CASCADE,
    sequence              BIGINT NOT NULL,
    operation_id          UUID NOT NULL,
    request_digest        TEXT NOT NULL,
    authoring_model_ref   TEXT NOT NULL,
    state                 TEXT NOT NULL,
    user_text             TEXT NOT NULL,
    answer_text           TEXT,
    candidate_id          UUID,
    usage                 JSONB,
    failure_reason        TEXT,
    finished_at           BIGINT,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL,
    CONSTRAINT uq_authoring_messages_sequence UNIQUE (draft_id, sequence),
    CONSTRAINT uq_authoring_messages_operation UNIQUE (draft_id, operation_id)
);

-- Claim scans states supplied by application constants; no duplicated enum
-- literal in a partial-index predicate. The sequence unique serves reads.
CREATE INDEX IF NOT EXISTS idx_authoring_messages_pending
    ON core.authoring_messages(state, created_at, id);
GRANT SELECT, INSERT, UPDATE, DELETE ON core.authoring_messages TO api_runtime;
