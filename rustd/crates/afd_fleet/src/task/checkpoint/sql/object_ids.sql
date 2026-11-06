SELECT refs.task_object_id::text
FROM (
    SELECT r.task_object_id FROM fleet.task_object_refs r
    WHERE r.task_checkpoint_id = $1::uuid AND r.workspace_id = $3::uuid AND ($4::uuid IS NULL OR r.task_object_id > $4::uuid)
    UNION
    SELECT r.task_object_id FROM fleet.task_object_refs r
    WHERE r.task_base_id = $2::uuid AND r.workspace_id = $3::uuid AND ($4::uuid IS NULL OR r.task_object_id > $4::uuid)
) refs ORDER BY refs.task_object_id LIMIT $5
