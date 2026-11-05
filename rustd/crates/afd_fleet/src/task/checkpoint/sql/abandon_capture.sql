UPDATE fleet.task_checkpoints SET abandoned_at = clock_timestamp(), updated_at = now()
WHERE task_id = $1::uuid AND ready_at IS NULL AND abandoned_at IS NULL
  AND (owner_lease_id <> $2::uuid OR owner_fence <> $3 OR transfer_expires_at <= clock_timestamp())
