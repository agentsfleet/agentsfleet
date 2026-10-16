-- Runtime is cumulative across renewal/report, independently of provider usage.
-- Canonical initial meter: afd_authoring::lease::meter::INITIAL_RUNTIME_MS.
ALTER TABLE fleet.authoring_attempts
    ADD COLUMN IF NOT EXISTS reported_runtime_ms BIGINT NOT NULL DEFAULT 0;

-- An unresolved report can acknowledge accounting without declaring it settled.
-- Its content-free identity survives deletion and never reopens private text.
CREATE TABLE IF NOT EXISTS billing.authoring_report_receipts (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_report_receipts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    attempt_id          UUID NOT NULL REFERENCES fleet.authoring_attempts(id) ON DELETE CASCADE,
    report_id           UUID NOT NULL,
    accounting_digest   TEXT NOT NULL,
    acknowledgement     JSONB NOT NULL,
    created_at          BIGINT NOT NULL,
    CONSTRAINT uq_authoring_report_receipts_identity UNIQUE (attempt_id, report_id)
);
GRANT SELECT, INSERT ON billing.authoring_report_receipts TO api_runtime;

-- Optional answer submission has a separate, expiring private replay identity.
-- The actual answer remains only in authoring_messages, never in this receipt.
CREATE TABLE IF NOT EXISTS core.authoring_answer_receipts (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_answer_receipts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    draft_id            UUID NOT NULL REFERENCES core.authoring_drafts(id) ON DELETE CASCADE,
    attempt_id          UUID NOT NULL REFERENCES fleet.authoring_attempts(id) ON DELETE CASCADE,
    report_id           UUID NOT NULL,
    answer_digest       TEXT NOT NULL,
    expires_at          BIGINT NOT NULL,
    created_at          BIGINT NOT NULL,
    CONSTRAINT uq_authoring_answer_receipts_identity UNIQUE (attempt_id, report_id)
);
CREATE INDEX IF NOT EXISTS idx_authoring_answer_receipts_expiry
    ON core.authoring_answer_receipts(expires_at, id);
GRANT SELECT, INSERT, DELETE ON core.authoring_answer_receipts TO api_runtime;
