SELECT id::text AS id, runner_id::text AS runner_id, event_type, created_at,
       metadata::text AS metadata
FROM runner.events
WHERE runner_id = $1::uuid
  AND ($2::text[] IS NULL OR event_type = ANY($2::text[]))
  AND ($3::timestamptz IS NULL OR created_at >= $3::timestamptz)
  AND ($4::timestamptz IS NULL OR created_at <= $4::timestamptz)
  AND (created_at, id) < ($5::timestamptz, $6::uuid)
ORDER BY created_at DESC, id DESC
LIMIT $7::bigint
