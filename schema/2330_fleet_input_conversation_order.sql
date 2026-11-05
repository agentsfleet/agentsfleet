-- Exact conversation equality and sequence pagination avoid scanning another
-- conversation's backlog. Settled history stays outside the working index.
ALTER TABLE fleet.admissions ADD COLUMN interpreted_at TIMESTAMPTZ(3);
CREATE INDEX idx_admissions_pending_conversation
    ON fleet.admissions (fleet_id, reply_provider, reply_address, sequence)
    WHERE settled_at IS NULL;
CREATE INDEX idx_admissions_uninterpreted_order ON fleet.admissions (fleet_id, sequence)
    WHERE settled_at IS NULL AND interpreted_at IS NULL;
CREATE INDEX idx_admissions_task_pending ON fleet.admissions (task_id)
    WHERE task_id IS NOT NULL AND settled_at IS NULL;
CREATE INDEX idx_admissions_task_item ON fleet.admissions (task_item_id)
    WHERE task_item_id IS NOT NULL;
