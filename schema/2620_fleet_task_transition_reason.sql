-- Bounded native reasons explain waits and blockers without retaining model text.
ALTER TABLE fleet.tasks ADD COLUMN status_reason TEXT,
    ADD COLUMN last_transition_at TIMESTAMPTZ(3);
-- A partial containment index uses the native target itself as its sole source.
CREATE INDEX idx_tasks_dependency_target ON fleet.tasks USING gin (target jsonb_path_ops)
    WHERE terminal_at IS NULL;
CREATE INDEX idx_tasks_unfinished_attempt_limit ON fleet.tasks (id)
    WHERE terminal_at IS NULL AND used_leases >= max_leases;
