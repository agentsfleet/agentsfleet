-- Closes the fleet ↔ revision cycle. Deferred, so Save inserts the fleet, its first
-- revision and the pointer in one transaction, checked at commit. Purge cascades
-- the revisions; the pointer's own row is going in the same statement.

ALTER TABLE fleet.fleets
    ADD CONSTRAINT fk_fleets_head_revision
    FOREIGN KEY (head_revision_id) REFERENCES fleet.revisions(id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX idx_fleets_head_revision_id
    ON fleet.fleets (head_revision_id) WHERE head_revision_id IS NOT NULL;
