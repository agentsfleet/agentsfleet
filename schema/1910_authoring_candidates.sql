-- A frozen, numbered setup the model proposed in a chat. A table, not a draft
-- column, because a chat proposes many and a turn points at the one it produced.
-- Append-only (rule 9); it goes with its draft (deletion matrix: Draft).

CREATE TABLE authoring.candidates (
    id                UUID PRIMARY KEY,
    CONSTRAINT ck_candidates_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    draft_id          UUID   NOT NULL REFERENCES authoring.drafts(id) ON DELETE CASCADE,
    revision          BIGINT NOT NULL,
    definition        JSONB  NOT NULL,
    canonical_digest  TEXT   NOT NULL,
    policy_semantics  TEXT   NOT NULL,
    created_at        BIGINT NOT NULL,
    CONSTRAINT uq_candidates_draft_id_revision UNIQUE (draft_id, revision)
);

CREATE FUNCTION authoring.candidates_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'candidates is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_candidates_refuse_update
    BEFORE UPDATE ON authoring.candidates
    FOR EACH ROW EXECUTE FUNCTION authoring.candidates_refuse_update();

GRANT SELECT, INSERT, DELETE ON authoring.candidates TO api_runtime;
