UPDATE fleet.task_object_pins SET released_at = clock_timestamp(), updated_at = now()
WHERE task_checkpoint_id = $1::uuid AND kind = $2 AND released_at IS NULL
  AND (owner_lease_id <> $3::uuid OR owner_fence <> $4)
