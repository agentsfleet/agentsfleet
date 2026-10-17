-- Frozen setup history: revision 1 at Save, +1 per Apply. Append-only (rule 9).
--
-- `review_context` is the frozen review subject (creator, source pin, instance
-- name) activation re-reviews against: a Library install has no draft, and a swept
-- draft cannot supply it. `review_digest` is the digest that review signed.
-- `runtime_model_ref` is the model resolved at Save or Apply, because a definition
-- may omit its model and review then resolves the tenant default.
--
-- `source_candidate_id` carries no foreign key: candidates are swept with their
-- draft after 30 days, and the history must outlive them.

CREATE TABLE fleet.revisions (
    id                   UUID PRIMARY KEY,
    CONSTRAINT ck_revisions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id             UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    revision             BIGINT NOT NULL,
    definition           JSONB  NOT NULL,
    canonical_digest     TEXT   NOT NULL,
    policy_semantics     TEXT   NOT NULL,
    runtime_model_ref    TEXT,
    review_digest        TEXT,
    review_context       JSONB,
    source_candidate_id  UUID,
    created_at           BIGINT NOT NULL,
    CONSTRAINT uq_revisions_fleet_id_revision UNIQUE (fleet_id, revision)
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
