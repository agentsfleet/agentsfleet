SELECT reply.response_text FROM fleet.task_items i
LEFT JOIN LATERAL (
    SELECT response_text FROM fleet.task_input_receipts r
    WHERE r.task_item_id = i.id AND r.response_text IS NOT NULL ORDER BY r.created_at LIMIT 1
) reply ON true
WHERE i.admission_id = $1 AND i.fleet_id = $2 AND i.workspace_id = $3
ORDER BY i.ordinal
