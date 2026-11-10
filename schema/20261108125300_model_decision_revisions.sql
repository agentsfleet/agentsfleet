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

-- Match the identity FK's read/lock authority without granting UPDATE on
-- immutable revisions. Current rows allow intermediate changes or deletion
-- in the same transaction; the selected revision stays locked through commit.
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

-- The existing catalogue selects a reviewed decision definition separately
-- from its coding transport. The boolean can revoke dispatch without changing
-- immutable definitions or erasing old call accounting.
ALTER TABLE model.platform_models
    ADD CONSTRAINT fk_platform_models_decision_revision FOREIGN KEY (decision_revision_id)
        REFERENCES model.decision_revisions (id)
        DEFERRABLE INITIALLY DEFERRED;
