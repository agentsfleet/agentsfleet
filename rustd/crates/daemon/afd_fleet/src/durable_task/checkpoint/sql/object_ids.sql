SELECT refs.durable_task_object_id
FROM (
    SELECT r.durable_task_object_id FROM fleet.durable_task_object_refs r
    WHERE r.durable_task_checkpoint_id = $1 AND r.workspace_id = $3 AND (r.durable_task_object_id > $4 OR $4 IS NULL)
    UNION
    SELECT r.durable_task_object_id FROM fleet.durable_task_object_refs r
    WHERE r.durable_task_base_id = $2 AND r.workspace_id = $3 AND (r.durable_task_object_id > $4 OR $4 IS NULL)
) refs ORDER BY refs.durable_task_object_id LIMIT $5
