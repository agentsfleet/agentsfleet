-- One page, after $4, of the objects checkpoint $1, a generation its pages are a delta through,
-- or its retained base $2 names.
WITH RECURSIVE chain (id) AS (
    SELECT $1::uuid
    UNION ALL
    SELECT c.parent_checkpoint_id
    FROM runner.checkpoints c JOIN chain ON c.id = chain.id
    WHERE c.parent_checkpoint_id IS NOT NULL
)
SELECT refs.checkpoint_object_id
FROM (
    SELECT r.checkpoint_object_id FROM runner.checkpoint_object_refs r
    WHERE r.checkpoint_id IN (SELECT id FROM chain) AND r.workspace_id = $3
      AND (r.checkpoint_object_id > $4 OR $4 IS NULL)
    UNION
    SELECT r.checkpoint_object_id FROM runner.checkpoint_object_refs r
    WHERE r.checkpoint_base_id = $2 AND r.workspace_id = $3 AND (r.checkpoint_object_id > $4 OR $4 IS NULL)
) refs ORDER BY refs.checkpoint_object_id LIMIT $5
