WITH allocated AS (
    UPDATE fleet.durable_tasks SET wait_sequence = COALESCE(wait_sequence, 0) + 1, updated_at = now()
    WHERE id = $2 RETURNING id, fleet_id, workspace_id, wait_sequence
)
INSERT INTO fleet.durable_task_waits
    (id, durable_task_id, fleet_id, workspace_id, owner_run_id, generation, expected_head, checks,
     created_at, updated_at)
SELECT $1, a.id, a.fleet_id, a.workspace_id, $3, a.wait_sequence, $4, $5, now(), now()
FROM allocated a
