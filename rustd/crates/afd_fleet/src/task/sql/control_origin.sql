SELECT i.admission_id::text AS source_admission_id, t.generation
FROM fleet.tasks t JOIN fleet.task_items i ON i.id = t.originating_task_item_id
WHERE t.id = $1::uuid AND t.fleet_id = $2::uuid AND t.workspace_id = $3::uuid
