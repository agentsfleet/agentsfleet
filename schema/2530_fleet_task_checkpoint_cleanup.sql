-- checkpoint/maintenance reads this indexed due marker; no queue/dispatcher is added.
ALTER TABLE fleet.tasks ADD COLUMN checkpoint_cleanup_at TIMESTAMPTZ(3);
CREATE INDEX idx_task_checkpoint_cleanup ON fleet.tasks (checkpoint_cleanup_at, id)
    WHERE checkpoint_cleanup_at IS NOT NULL;
