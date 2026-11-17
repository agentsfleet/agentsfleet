-- Frozen setup history: revision 1 at Save, +1 per Apply; append-only. `source_candidate_id` has
-- no foreign key: candidates are swept with their draft, and this history must outlive them.
CREATE TABLE fleet.revisions (
    id                   UUID PRIMARY KEY,
    CONSTRAINT ck_revisions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id             UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    revision             BIGINT NOT NULL,
    definition           JSONB  NOT NULL,
    canonical_digest     TEXT   NOT NULL,
    policy_semantics     TEXT   NOT NULL,
    -- Resolved at Save or Apply, because a definition may omit its model.
    runtime_model_ref    TEXT,
    review_digest        TEXT,
    -- The frozen subject activation re-reviews against; a Plugin install has no draft to supply it.
    review_context       JSONB,
    source_candidate_id  UUID,
    created_at           TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_revisions_fleet_id_revision UNIQUE (fleet_id, revision),
    CONSTRAINT uq_revisions_id_fleet UNIQUE (id, fleet_id)
);

CREATE FUNCTION fleet.revisions_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'revisions is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_revisions_refuse_update
    BEFORE UPDATE ON fleet.revisions
    FOR EACH ROW EXECUTE FUNCTION fleet.revisions_refuse_update();

GRANT SELECT, INSERT, DELETE ON fleet.revisions TO api_runtime;

-- Deferred, so Save inserts the fleet, its first revision and the pointer in one transaction.
ALTER TABLE fleet.fleets
    ADD CONSTRAINT fk_fleets_head_revision FOREIGN KEY (head_revision_id) REFERENCES fleet.revisions(id)
    DEFERRABLE INITIALLY DEFERRED;
