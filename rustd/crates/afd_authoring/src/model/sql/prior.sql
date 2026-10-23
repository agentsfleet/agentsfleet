SELECT id::text AS id,
       sequence,
       request_digest,
       status,
       result::text AS result,
       dispatch_expires_at
  FROM authoring.dispatches
 WHERE attempt_id = $1::uuid AND (id = $2::uuid OR sequence = $3)
