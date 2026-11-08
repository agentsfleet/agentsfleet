SELECT i.task_id, t.generation, t.title, n.sequence, t.covered_input_sequence, t.status AS state
FROM fleet.task_items i
LEFT JOIN fleet.task_interpretations p ON p.admission_id = i.admission_id
    AND p.fleet_id = i.fleet_id AND p.workspace_id = i.workspace_id
LEFT JOIN fleet.tasks t ON t.id = i.task_id AND t.fleet_id = i.fleet_id AND t.workspace_id = i.workspace_id
LEFT JOIN fleet.task_inputs n ON n.task_item_id = i.id AND n.task_id = i.task_id
WHERE i.id = $1 AND i.fleet_id = $2 AND i.workspace_id = $3
  AND i.admission_id = $4 AND i.plan_digest = $5
  AND (p.source_producer IS DISTINCT FROM $6
       OR connector.slack_person_allowed(i.workspace_id, p.source_actor, $7, $8, $9))
