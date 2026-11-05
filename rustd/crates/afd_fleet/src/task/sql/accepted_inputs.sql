SELECT i.admission_id::text AS source_admission_id, i.plan_digest, i.id::text AS item_id,
       t.id::text AS task_id, t.generation, n.sequence, i.action,
       p.message AS original_message, n.accepted_text, n.interpretation,
       n.sequence <= t.covered_input_sequence AS applied,
       (SELECT r.response_text FROM fleet.task_input_receipts r WHERE r.task_item_id = i.id AND r.response_text IS NOT NULL) AS response_text
FROM fleet.tasks t
LEFT JOIN fleet.task_checkpoints c ON c.id = t.task_checkpoint_id AND c.task_id = t.id
JOIN fleet.task_inputs n ON n.task_id = t.id AND n.fleet_id = t.fleet_id AND n.workspace_id = t.workspace_id
JOIN fleet.task_items i ON i.id = n.task_item_id AND i.fleet_id = n.fleet_id AND i.workspace_id = n.workspace_id
JOIN fleet.task_interpretations p ON p.admission_id = i.admission_id AND p.plan_digest = i.plan_digest
    AND p.fleet_id = i.fleet_id AND p.workspace_id = i.workspace_id
WHERE t.id = $1::uuid AND t.fleet_id = $2::uuid AND t.workspace_id = $3::uuid
  AND n.sequence > COALESCE($5::bigint, c.covered_input_sequence, 0)
ORDER BY n.sequence LIMIT $4
