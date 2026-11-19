SELECT id, run_fence, task_id, task_generation,
       parked_at IS NOT NULL AS parked, settled_at IS NOT NULL AS settled
FROM fleet.admissions
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3
FOR UPDATE
