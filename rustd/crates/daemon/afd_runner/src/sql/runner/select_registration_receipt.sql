SELECT request_digest, runner_id
FROM runner.registration_receipts
WHERE tenant_id = $1 AND operation_id = $2
