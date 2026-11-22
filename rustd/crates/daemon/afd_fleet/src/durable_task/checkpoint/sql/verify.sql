WITH locked AS MATERIALIZED (
    SELECT o.id FROM runner.checkpoint_objects o
    JOIN jsonb_to_recordset($2::jsonb) AS n(id uuid, digest text, length bigint)
      ON n.id = o.id AND n.digest = o.ciphertext_digest AND n.length = o.ciphertext_length
    WHERE o.workspace_id = $1 AND o.verified_at IS NULL AND o.tombstoned_at IS NULL
    ORDER BY o.id FOR UPDATE OF o
)
UPDATE runner.checkpoint_objects o SET verified_at = clock_timestamp(), updated_at = now()
FROM locked WHERE o.id = locked.id
