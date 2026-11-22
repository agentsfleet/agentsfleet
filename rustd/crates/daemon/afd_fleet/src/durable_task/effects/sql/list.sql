SELECT e.id, e.durable_task_id, e.fleet_id, e.workspace_id,
       e.durable_task_generation, e.stop_generation, e.ordinal, e.expected_head, e.tested_tree,
       e.request_digest, p.target, s.status AS state, s.commit_oid
FROM fleet.durable_task_effects e JOIN fleet.durable_task_effect_states s ON s.durable_task_effect_id = e.id
LEFT JOIN fleet.durable_task_effect_payloads p ON p.durable_task_effect_id = e.id
WHERE e.durable_task_id = $1 AND e.fleet_id = $2 AND e.workspace_id = $3 AND e.ordinal > $4
ORDER BY e.ordinal LIMIT $5
