SELECT id::text AS id,
       sequence,
       request_digest,
       status,
       result::text AS result,
       created_at
  FROM authoring.dispatches
 WHERE attempt_id = $1::uuid AND status <> $2
 ORDER BY sequence
 LIMIT 1
