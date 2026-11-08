SELECT EXISTS (SELECT 1 FROM fleet.task_checkpoints c
WHERE c.id = $1::uuid AND c.task_id = $2::uuid AND c.owner_lease_id = $3::uuid
  AND c.task_generation = $4 AND c.stop_generation = $5 AND c.accepted_revision_id = $6::uuid
  AND c.ready_at IS NOT NULL AND c.abandoned_at IS NULL
  AND c.covered_input_sequence = $7 AND c.effect_watermark = $8
  AND ($9::text IS NULL OR c.commit_oid = $9))
