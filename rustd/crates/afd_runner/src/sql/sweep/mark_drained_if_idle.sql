WITH updated AS (
  UPDATE runner.runners r
  SET status = $2::text, updated_at = now()
  WHERE r.id = $1::uuid AND r.status = $3::text
    AND NOT EXISTS (
      SELECT 1 FROM runner.leases l
      WHERE l.runner_id = r.id AND l.status = $4::text
    )
  RETURNING r.id
), inserted AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $5::uuid, id, $6::text,
         jsonb_build_object($7::text, $3::text, $8::text, $2::text), NULL, now()
  FROM updated
  RETURNING 1
)
SELECT COUNT(*)::bigint FROM inserted
