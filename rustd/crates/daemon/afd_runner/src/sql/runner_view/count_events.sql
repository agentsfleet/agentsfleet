SELECT COUNT(*)::bigint FROM runner.events
WHERE runner_id = $1::uuid
  AND ($2::text[] IS NULL OR event_type = ANY($2::text[]))
  AND ($3::timestamptz IS NULL OR created_at >= $3::timestamptz)
  AND ($4::timestamptz IS NULL OR created_at <= $4::timestamptz)
