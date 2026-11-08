INSERT INTO authoring.dispatches
    (id, attempt_id, sequence, request_digest, status, private_expires_at,
     dispatch_expires_at, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4, $5, clock_timestamp() + $6::interval,
        $7::timestamptz, now(), now())
ON CONFLICT DO NOTHING
