SELECT e.id::text, e.task_id::text, e.fleet_id::text, e.workspace_id::text,
       e.task_generation, e.stop_generation, e.ordinal, e.expected_head, e.tested_tree,
       e.request_digest, p.target, s.status AS state, s.commit_oid
FROM fleet.task_effects e JOIN fleet.task_effect_states s ON s.task_effect_id = e.id
LEFT JOIN fleet.task_effect_payloads p ON p.task_effect_id = e.id
WHERE e.task_id = $1::uuid AND e.fleet_id = $2::uuid AND e.workspace_id = $3::uuid AND e.ordinal > $4
ORDER BY e.ordinal LIMIT $5
