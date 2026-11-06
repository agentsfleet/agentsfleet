-- Input association and the current-work list must not scan terminal history
-- to discover at most sixteen unfinished objectives in creation order.
CREATE INDEX idx_tasks_unfinished_created
    ON fleet.tasks (fleet_id, created_at DESC, id DESC)
    WHERE terminal_at IS NULL;
