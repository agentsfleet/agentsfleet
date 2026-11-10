SELECT refs.task_object_id
FROM (
    SELECT r.task_object_id FROM fleet.task_object_refs r
    WHERE r.task_checkpoint_id = $1 AND r.workspace_id = $3 AND (r.task_object_id > $4 OR $4 IS NULL)
    UNION
    SELECT r.task_object_id FROM fleet.task_object_refs r
    WHERE r.task_base_id = $2 AND r.workspace_id = $3 AND (r.task_object_id > $4 OR $4 IS NULL)
) refs ORDER BY refs.task_object_id LIMIT $5
