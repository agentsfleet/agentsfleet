-- Arms the task's next wait. A live wait is never replaced: the owning transition cancels it
-- first, so finding one answers no row and fails the transition.
UPDATE fleet.durable_tasks SET wait_sequence = COALESCE(wait_sequence, 0) + 1,
    wait_owner_run_id = $2, wait_expected_head = $3, wait_checks = $4, wait_cancelled_at = NULL,
    wait_updated_at = now(), updated_at = now()
WHERE id = $1 AND (wait_sequence IS NULL OR wait_cancelled_at IS NOT NULL)
RETURNING wait_sequence
