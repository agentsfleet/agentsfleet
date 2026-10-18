UPDATE authoring.attempts
   SET status = $2,
       usage_unknown = $3,
       reported_runtime_ms = $4,
       lease_expires_at = LEAST(lease_expires_at, $5::timestamptz),
       outcome = $6,
       updated_at = now()
 WHERE id = $1::uuid
