-- A frozen published version of a Plugin card. A table, not card columns, because
-- a draft and an install pin one exact version while the card moves on.
-- Append-only (rule 9); deleting the card cascades its revisions.
--
-- The definition is copied onto each frozen revision on purpose: a shared
-- definitions table would turn deletion from a cascade into a reference-counted
-- sweep. `canonical_digest` and `policy_semantics` are read by the install review.
CREATE TABLE authoring.plugin_revisions (
    id                UUID PRIMARY KEY,
    CONSTRAINT ck_plugin_revisions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    plugin_entry_id  UUID   NOT NULL REFERENCES authoring.plugin_entries(id) ON DELETE CASCADE,
    revision          BIGINT NOT NULL,
    definition        JSONB  NOT NULL,
    canonical_digest  TEXT   NOT NULL,
    policy_semantics  TEXT   NOT NULL,
    created_at        TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_plugin_revisions_plugin_entry_id_revision UNIQUE (plugin_entry_id, revision)
);

CREATE FUNCTION authoring.plugin_revisions_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'plugin_revisions is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_plugin_revisions_refuse_update
    BEFORE UPDATE ON authoring.plugin_revisions
    FOR EACH ROW EXECUTE FUNCTION authoring.plugin_revisions_refuse_update();

GRANT SELECT, INSERT, DELETE ON authoring.plugin_revisions TO api_runtime;

-- Closes the card ↔ revision cycle: a card points at its live revision, and a
-- revision belongs to its card. Deferred, so publishing inserts the revision and
-- moves the pointer in one transaction, checked at commit.
ALTER TABLE authoring.plugin_entries
    ADD CONSTRAINT fk_plugin_entries_head_revision FOREIGN KEY (head_revision_id) REFERENCES authoring.plugin_revisions(id)
    DEFERRABLE INITIALLY DEFERRED;
