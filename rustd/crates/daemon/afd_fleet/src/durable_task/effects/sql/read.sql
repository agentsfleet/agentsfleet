SELECT e.id, e.durable_task_id, e.fleet_id, e.workspace_id,
       e.durable_task_generation, e.stop_generation, e.ordinal, e.expected_head, e.tested_tree,
       e.request_digest, e.target, e.status AS state, e.commit_oid
FROM fleet.durable_task_effects e
WHERE e.id = $1 AND e.fleet_id = $2 AND e.workspace_id = $3 AND e.durable_task_id = $4
