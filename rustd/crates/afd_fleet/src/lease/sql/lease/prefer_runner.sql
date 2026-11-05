INSERT INTO runner.fleet_slots AS s
  (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
   ready_at, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, 0, clock_timestamp() - interval '1 millisecond', NULL,
        NULL, now(), now())
ON CONFLICT (fleet_id) DO UPDATE
  SET last_runner_id = EXCLUDED.last_runner_id, updated_at = now()
