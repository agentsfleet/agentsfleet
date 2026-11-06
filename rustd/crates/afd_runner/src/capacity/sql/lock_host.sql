-- NO KEY UPDATE: capacity lockers still exclude each other, but a foreign-key
-- check on this runner (KEY SHARE) does not wait, so a statement holding a
-- fleet slot can recheck `last_runner_id` while a renewal holds the host.
SELECT worker_count, status, degraded_reason IS NOT NULL AS degraded, lease_version
FROM runner.runners
WHERE id = $1::uuid
FOR NO KEY UPDATE
