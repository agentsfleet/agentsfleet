SELECT e.id, e.durable_task_id, e.fleet_id, e.workspace_id,
       e.durable_task_generation, e.stop_generation, e.ordinal, e.expected_head, e.tested_tree,
       e.request_digest, e.target, e.status AS state, e.commit_oid
FROM fleet.durable_task_effects e
WHERE e.durable_task_id = $1 AND e.operation_id = $2
