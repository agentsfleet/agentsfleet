-- Each accepted edit creates a fresh immutable snapshot. The draft points at
-- its head; proposals and their replay receipts retain the accepted identity.
CREATE TABLE IF NOT EXISTS core.authoring_candidates (
    id                    UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_candidates_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    draft_id              UUID NOT NULL REFERENCES core.authoring_drafts(id) ON DELETE CASCADE,
    revision              BIGINT NOT NULL,
    definition            JSONB NOT NULL,
    canonical_digest      TEXT NOT NULL,
    policy_semantics      TEXT NOT NULL,
    created_at            BIGINT NOT NULL,
    CONSTRAINT uq_authoring_candidates_revision UNIQUE (draft_id, revision)
);

-- UPDATE is intentionally absent. DELETE is the privacy boundary and the
-- draft cascade; installed/published setups own their independent snapshots.
GRANT SELECT, INSERT, DELETE ON core.authoring_candidates TO api_runtime;
