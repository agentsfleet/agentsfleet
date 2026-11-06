-- The existing catalogue selects a reviewed decision definition separately
-- from its coding transport. The boolean can revoke dispatch without changing
-- immutable definitions or erasing old call accounting.
ALTER TABLE model.platform_models
    ADD COLUMN decision_revision_id UUID,
    ADD COLUMN decision_enabled BOOLEAN NOT NULL DEFAULT false,
    ADD CONSTRAINT fk_platform_models_decision_revision
        FOREIGN KEY (decision_revision_id)
        REFERENCES model.decision_revisions (id)
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT ck_platform_models_decision_activation
        CHECK (decision_revision_id IS NOT NULL OR NOT decision_enabled);

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
