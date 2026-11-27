WITH locked AS MATERIALIZED (
    SELECT o.id FROM runner.checkpoint_objects o
    WHERE o.id = ANY($1) AND o.workspace_id = $2
      AND o.owner_checkpoint_id = $3 AND o.owner_run_id = $4 AND o.owner_fence = $5
      AND o.ciphertext_digest IS NOT NULL AND o.verified_at IS NULL AND o.tombstoned_at IS NULL
    ORDER BY o.id FOR UPDATE OF o
)
UPDATE runner.checkpoint_objects o SET verified_at = clock_timestamp(), updated_at = now()
FROM locked WHERE o.id = locked.id
