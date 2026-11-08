SELECT e.id, e.task_id, e.fleet_id, e.workspace_id,
       e.task_generation, e.stop_generation, e.ordinal, e.expected_head, e.tested_tree,
       e.request_digest, p.target, s.status AS state, s.commit_oid
FROM fleet.task_effects e JOIN fleet.task_effect_states s ON s.task_effect_id = e.id
LEFT JOIN fleet.task_effect_payloads p ON p.task_effect_id = e.id
WHERE e.id = $1 AND e.fleet_id = $2 AND e.workspace_id = $3 AND e.task_id = $4
