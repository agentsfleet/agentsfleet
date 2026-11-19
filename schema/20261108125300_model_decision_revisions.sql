-- Append-only reviewed decision definitions for a catalogue model. `platform_models` selects the
-- current one; lease and call snapshots bind the same revision.
CREATE TABLE model.decision_revisions (
    id UUID PRIMARY KEY,
    platform_model_id UUID NOT NULL REFERENCES model.platform_models(id) ON DELETE CASCADE,
    revision TEXT NOT NULL,
    definition JSONB NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_decision_revisions_model_revision UNIQUE (platform_model_id, revision),
    -- Lets decision prices prove the owning model alongside the revision.
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

-- Stands in for a foreign key's lock without granting UPDATE on immutable revisions: the selected
-- revision stays KEY SHARE locked through commit.
CREATE FUNCTION model.enforce_decision_revision_owner() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER
SET search_path = pg_catalog, pg_temp AS $$
DECLARE
    selected_revision UUID;
BEGIN
    SELECT decision_revision_id INTO selected_revision
      FROM model.platform_models WHERE id = NEW.id;
    IF NOT FOUND OR selected_revision IS NULL THEN
        RETURN NULL;
    END IF;
    PERFORM 1 FROM model.decision_revisions
      WHERE id = selected_revision AND platform_model_id = NEW.id
      FOR KEY SHARE;
    IF NOT FOUND THEN
        RAISE foreign_key_violation
            USING MESSAGE = TG_NAME, CONSTRAINT = TG_NAME;
    END IF;
    RETURN NULL;
END;
$$;

REVOKE ALL ON FUNCTION model.enforce_decision_revision_owner() FROM PUBLIC;

CREATE CONSTRAINT TRIGGER fk_platform_models_decision_revision_owner
    AFTER INSERT OR UPDATE ON model.platform_models
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION model.enforce_decision_revision_owner();

ALTER TABLE model.platform_models
    ADD CONSTRAINT fk_platform_models_decision_revision FOREIGN KEY (decision_revision_id)
        REFERENCES model.decision_revisions (id)
        DEFERRABLE INITIALLY DEFERRED;
