-- Queue ownership freezes the task generation separately from a lease fence.
-- Retain this marker if privacy cleanup removes its nullable task reference.
ALTER TABLE fleet.admissions ADD COLUMN task_generation BIGINT CHECK (task_generation > 0);
CREATE INDEX idx_admissions_task_generation ON fleet.admissions (task_id, task_generation, settled_at, sequence)
    WHERE task_id IS NOT NULL;
