SELECT i.id::text AS item_id, i.task_id::text AS task_id, i.plan_digest
FROM fleet.task_items i
WHERE i.admission_id = $1::uuid AND i.fleet_id = $2::uuid AND i.workspace_id = $3::uuid
ORDER BY i.ordinal
