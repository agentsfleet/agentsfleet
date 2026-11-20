-- A frozen, numbered setup the model proposed in a chat. Append-only; it goes with its draft.
CREATE TABLE setup.candidates (
    id                UUID PRIMARY KEY,
    draft_id          UUID   NOT NULL REFERENCES setup.drafts(id) ON DELETE CASCADE,
    revision          BIGINT NOT NULL,
    definition        JSONB  NOT NULL,
    canonical_digest  TEXT   NOT NULL,
    policy_semantics  TEXT   NOT NULL,
    created_at        TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_candidates_draft_id_revision UNIQUE (draft_id, revision)
);

CREATE FUNCTION setup.candidates_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'candidates is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_candidates_refuse_update
    BEFORE UPDATE ON setup.candidates
    FOR EACH ROW EXECUTE FUNCTION setup.candidates_refuse_update();

GRANT SELECT, INSERT, DELETE ON setup.candidates TO api_runtime;

-- Deferred, so a turn inserts its candidate and moves the pointer in one transaction.
ALTER TABLE setup.drafts
    ADD CONSTRAINT fk_drafts_head_candidate FOREIGN KEY (head_candidate_id) REFERENCES setup.candidates(id)
    DEFERRABLE INITIALLY DEFERRED;
