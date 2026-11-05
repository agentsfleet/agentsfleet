SELECT refs.object_id::text
FROM (
    SELECT r.object_id FROM fleet.task_object_refs r
    WHERE r.checkpoint_id = $1::uuid AND r.workspace_id = $3::uuid AND ($4::uuid IS NULL OR r.object_id > $4::uuid)
    UNION
    SELECT r.object_id FROM fleet.task_object_refs r
    WHERE r.base_id = $2::uuid AND r.workspace_id = $3::uuid AND ($4::uuid IS NULL OR r.object_id > $4::uuid)
) refs ORDER BY refs.object_id LIMIT $5
