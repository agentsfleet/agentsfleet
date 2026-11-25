SELECT EXISTS (SELECT 1 FROM runner.checkpoints c
WHERE c.id = $1 AND c.durable_task_id = $2 AND c.owner_run_id = $3
  AND c.durable_task_generation = $4 AND c.stop_generation = $5
  AND c.ready_at IS NOT NULL AND c.abandoned_at IS NULL
  AND c.covered_input_sequence = $6 AND c.effect_watermark = $7
  AND ($8::text IS NULL OR c.commit_oid = $8))
