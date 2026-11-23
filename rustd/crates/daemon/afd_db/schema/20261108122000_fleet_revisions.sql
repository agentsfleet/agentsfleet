-- Every setup a chat proposed or a fleet ran. A proposal belongs to its draft (`draft_id`), numbered
-- per draft, and goes with it; Save or Apply installs it into its fleet with no copy, renumbering it
-- 1 at Save, +1 per Apply, and from then on it belongs to the fleet (`fleet_id`). Exactly one owner
-- is set, so each numbering constraint binds only its own rows: NULLs are distinct in a UNIQUE.
-- `installed` marks a revision that became its fleet's head; nothing else changes after insert.
CREATE TABLE fleet.revisions (
    id                   UUID PRIMARY KEY,
    fleet_id             UUID   REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    -- Its foreign key closes a cycle, so it is added in `setup.drafts`' file.
    draft_id             UUID,
    revision             BIGINT NOT NULL,
    definition           JSONB  NOT NULL,
    canonical_digest     TEXT   NOT NULL,
    policy_semantics     TEXT   NOT NULL,
    installed            BOOLEAN NOT NULL,
    -- Resolved at Save or Apply, because a definition may omit its model.
    runtime_model_ref    TEXT,
    review_digest        TEXT,
    -- The frozen subject activation re-reviews against; a system setup has no draft to supply it.
    review_context       JSONB,
    created_at           TIMESTAMPTZ(3) NOT NULL,
    updated_at           TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_revisions_one_owner CHECK (num_nonnulls(draft_id, fleet_id) = 1),
    CONSTRAINT ck_revisions_installed_in_fleet CHECK (NOT installed OR fleet_id IS NOT NULL),
    CONSTRAINT uq_revisions_fleet_id_revision UNIQUE (fleet_id, revision),
    CONSTRAINT uq_revisions_draft_id_revision UNIQUE (draft_id, revision),
    CONSTRAINT uq_revisions_id_fleet UNIQUE (id, fleet_id)
);

-- The one update a revision takes: installing a draft's proposal into its fleet. The definition and
-- its digests never change.
CREATE FUNCTION fleet.revisions_refuse_update() RETURNS trigger AS $$
BEGIN
    IF OLD.fleet_id IS NULL AND NEW.fleet_id IS NOT NULL AND NEW.draft_id IS NULL
       AND NOT OLD.installed AND NEW.installed
       AND NEW.id = OLD.id AND NEW.definition = OLD.definition
       AND NEW.canonical_digest = OLD.canonical_digest
       AND NEW.policy_semantics = OLD.policy_semantics AND NEW.created_at = OLD.created_at THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'a revision changes only when it is installed';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_revisions_refuse_update
    BEFORE UPDATE ON fleet.revisions
    FOR EACH ROW EXECUTE FUNCTION fleet.revisions_refuse_update();

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.revisions TO api_runtime;

-- Deferred, so Save inserts the fleet, installs its first revision and sets the pointer in one
-- transaction.
ALTER TABLE fleet.fleets
    ADD CONSTRAINT fk_fleets_head_revision FOREIGN KEY (head_revision_id) REFERENCES fleet.revisions(id)
    DEFERRABLE INITIALLY DEFERRED;
