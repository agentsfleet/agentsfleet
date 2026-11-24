-- A person's edit of a locked task's completion statements: new questions and goal digest,
-- generation + 1, and `ready` again when the edit wakes it.
--
-- $1 task · $2 fleet · $3 workspace · $4 questions · $5 goal digest · $6 the version read under
-- the lock · $7 whether the edit wakes the task · $8 the ready status
UPDATE fleet.durable_tasks
SET questions = $4::jsonb, goal_digest = $5, generation = generation + 1,
    status = CASE WHEN $7 THEN $8 ELSE status END,
    resource_version = resource_version + 1, updated_at = now()
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3 AND resource_version = $6
RETURNING id
