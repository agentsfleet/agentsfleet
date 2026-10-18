SELECT worker_count, status, degraded_reason IS NOT NULL AS degraded, lease_version
FROM runner.runners
WHERE id = $1::uuid
FOR UPDATE
