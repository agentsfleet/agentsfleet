-- Append-only definitions for a catalogue model's reviewed decision revisions.
-- Current selection lives on platform_models; lease/call snapshots bind the
-- same model/revision. Reusing a revision with different bytes is refused by
-- afd_admin before any configuration or schedule mutation can commit.
CREATE TABLE model.decision_revisions (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_decision_revisions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    platform_model_id UUID NOT NULL REFERENCES model.platform_models(id) ON DELETE CASCADE,
    revision TEXT NOT NULL,
    definition JSONB NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_decision_revisions_model_revision UNIQUE (platform_model_id, revision),
    -- Decision prices prove the owning model alongside their revision identity.
    CONSTRAINT uq_decision_revisions_id_model UNIQUE (id, platform_model_id)
);

CREATE FUNCTION model.decision_revisions_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'decision_revisions is append-only';
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_decision_revisions_refuse_update
    BEFORE UPDATE ON model.decision_revisions
    FOR EACH ROW EXECUTE FUNCTION model.decision_revisions_refuse_update();

GRANT SELECT, INSERT ON model.decision_revisions TO api_runtime;
