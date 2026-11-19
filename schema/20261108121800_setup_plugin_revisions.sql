-- Append-only. Each revision carries its own definition copy, so deleting a card stays a cascade
-- rather than a reference-counted sweep.
CREATE TABLE setup.plugin_revisions (
    id                UUID PRIMARY KEY,
    plugin_entry_id  UUID   NOT NULL REFERENCES setup.plugin_entries(id) ON DELETE CASCADE,
    revision          BIGINT NOT NULL,
    definition        JSONB  NOT NULL,
    canonical_digest  TEXT   NOT NULL,
    policy_semantics  TEXT   NOT NULL,
    created_at        TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_plugin_revisions_plugin_entry_id_revision UNIQUE (plugin_entry_id, revision)
);

CREATE FUNCTION setup.plugin_revisions_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'plugin_revisions is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_plugin_revisions_refuse_update
    BEFORE UPDATE ON setup.plugin_revisions
    FOR EACH ROW EXECUTE FUNCTION setup.plugin_revisions_refuse_update();

GRANT SELECT, INSERT, DELETE ON setup.plugin_revisions TO api_runtime;

-- Deferred, so a publish inserts the revision and moves the card's pointer in one transaction.
ALTER TABLE setup.plugin_entries
    ADD CONSTRAINT fk_plugin_entries_head_revision FOREIGN KEY (head_revision_id) REFERENCES setup.plugin_revisions(id)
    DEFERRABLE INITIALLY DEFERRED;
