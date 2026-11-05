WITH locked AS MATERIALIZED (
    SELECT o.id FROM fleet.task_objects o
    JOIN jsonb_to_recordset($2::jsonb) AS n(id uuid, digest text, length bigint)
      ON n.id = o.id AND n.digest = o.ciphertext_digest AND n.length = o.ciphertext_length
    WHERE o.workspace_id = $1::uuid AND o.verified_at IS NULL AND o.tombstoned_at IS NULL
    ORDER BY o.id FOR UPDATE OF o
), verified AS (
    UPDATE fleet.task_objects o SET verified_at = clock_timestamp(), updated_at = now()
    FROM locked WHERE o.id = locked.id RETURNING o.accounted_bytes
)
UPDATE fleet.task_storage_quota q
SET reserved_bytes = reserved_bytes - (SELECT COALESCE(sum(accounted_bytes), 0) FROM verified),
    stored_bytes = stored_bytes + (SELECT COALESCE(sum(accounted_bytes), 0) FROM verified), updated_at = now()
WHERE q.workspace_id = $1::uuid
