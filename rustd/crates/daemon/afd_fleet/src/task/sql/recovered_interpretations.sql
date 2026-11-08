SELECT i.admission_id AS source_admission_id, i.plan_digest, p.message, i.id AS item_id,
       i.task_id, t.generation, t.status AS state, n.sequence AS input_sequence,
       i.action, i.source_start, i.source_end,
       p.plan -> $4::text -> i.ordinal -> $5::text ->> $6::text AS interpretation,
       (SELECT r.response_text FROM fleet.task_input_receipts r
        WHERE r.task_item_id = i.id AND r.response_text IS NOT NULL) AS response_text
FROM fleet.task_interpretations p
JOIN fleet.task_items i ON i.admission_id = p.admission_id AND i.fleet_id = p.fleet_id AND i.workspace_id = p.workspace_id
LEFT JOIN fleet.tasks t ON t.id = i.task_id AND t.fleet_id = i.fleet_id AND t.workspace_id = i.workspace_id
LEFT JOIN fleet.task_inputs n ON n.task_item_id = i.id AND n.task_id = i.task_id
WHERE p.admission_id = ANY($1) AND p.fleet_id = $2 AND p.workspace_id = $3
ORDER BY i.admission_id, i.ordinal
