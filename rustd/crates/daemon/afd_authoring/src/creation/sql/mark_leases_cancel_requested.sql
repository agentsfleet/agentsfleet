UPDATE runner.leases
SET cancel_requested_at = COALESCE(cancel_requested_at, clock_timestamp()), updated_at = now()
WHERE fleet_id = $1 AND status = $2
RETURNING runner_id, id
