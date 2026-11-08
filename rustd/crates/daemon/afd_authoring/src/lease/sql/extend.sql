UPDATE authoring.attempts
   SET lease_expires_at = LEAST($2::timestamptz + $3::interval, expires_at),
       reported_runtime_ms = $4,
       updated_at = now()
 WHERE id = $1::uuid AND lease_expires_at > $2::timestamptz AND expires_at > $2::timestamptz
RETURNING lease_expires_at
