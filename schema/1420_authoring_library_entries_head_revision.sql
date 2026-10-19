-- Closes the card ↔ revision cycle: a card points at its live revision, and a
-- revision belongs to its card. Deferred, so publishing inserts the revision and
-- moves the pointer in one transaction, checked at commit.

ALTER TABLE authoring.library_entries
    ADD CONSTRAINT fk_library_entries_head_revision
    FOREIGN KEY (head_revision_id) REFERENCES authoring.library_revisions(id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX idx_library_entries_head_revision_id
    ON authoring.library_entries (head_revision_id) WHERE head_revision_id IS NOT NULL;
