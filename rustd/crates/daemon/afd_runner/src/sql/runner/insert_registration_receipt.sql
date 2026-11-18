INSERT INTO runner.registration_receipts
  (id, tenant_id, operation_id, request_digest, runner_id, created_at)
VALUES ($1, $2, $3, $4::text, $5, now())
ON CONFLICT (tenant_id, operation_id) DO NOTHING
RETURNING id
