SELECT refs.checkpoint_object_id
FROM (
    SELECT r.checkpoint_object_id FROM runner.checkpoint_object_refs r
    WHERE r.checkpoint_id = $1 AND r.workspace_id = $3 AND (r.checkpoint_object_id > $4 OR $4 IS NULL)
    UNION
    SELECT r.checkpoint_object_id FROM runner.checkpoint_object_refs r
    WHERE r.checkpoint_base_id = $2 AND r.workspace_id = $3 AND (r.checkpoint_object_id > $4 OR $4 IS NULL)
) refs ORDER BY refs.checkpoint_object_id LIMIT $5
