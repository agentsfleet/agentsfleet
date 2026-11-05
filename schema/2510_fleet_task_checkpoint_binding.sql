-- Close the task/checkpoint cycle after both owners exist.
ALTER TABLE fleet.tasks ADD COLUMN checkpoint_id UUID;
-- NULL means no plan allocated; native allocation increments this monotonic cursor.
ALTER TABLE fleet.tasks ADD COLUMN checkpoint_sequence BIGINT CHECK (checkpoint_sequence >= 0);
ALTER TABLE fleet.tasks ADD CONSTRAINT fk_task_checkpoint
    FOREIGN KEY (checkpoint_id, id, fleet_id, workspace_id)
    REFERENCES fleet.task_checkpoints (id, task_id, fleet_id, workspace_id)
    ON DELETE SET NULL (checkpoint_id) DEFERRABLE INITIALLY DEFERRED;
CREATE INDEX idx_task_checkpoint ON fleet.tasks (checkpoint_id) WHERE checkpoint_id IS NOT NULL;
