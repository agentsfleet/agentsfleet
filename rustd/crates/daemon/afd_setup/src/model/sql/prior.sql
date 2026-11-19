SELECT id,
       sequence,
       request_digest,
       status,
       result::text AS result,
       dispatch_expires_at
  FROM authoring.dispatches
 WHERE attempt_id = $1 AND (id = $2 OR sequence = $3)
