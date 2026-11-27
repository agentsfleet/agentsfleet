DELETE FROM runner.checkpoint_objects o
WHERE o.workspace_id = $1 AND o.repository = $2 AND o.content_identity = ANY($3::text[])
  AND o.tombstoned_at IS NULL AND o.verified_at IS NULL
  AND NOT (o.owner_checkpoint_id = $4 AND o.owner_run_id = $5 AND o.owner_fence = $6)
  AND NOT EXISTS (
      SELECT 1 FROM runner.checkpoint_pins p
      WHERE p.checkpoint_id = o.owner_checkpoint_id AND p.kind = $7
        AND p.expires_at > clock_timestamp())
