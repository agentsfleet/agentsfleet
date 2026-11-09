-- A broker Cancel identifies the row to inspect, never the authority to stop a lease.
-- The primary-key lookup also proves the addressed runner before the stream sends Cancel.
SELECT cancel_requested_at IS NOT NULL
FROM runner.leases
WHERE id = $1 AND runner_id = $2
