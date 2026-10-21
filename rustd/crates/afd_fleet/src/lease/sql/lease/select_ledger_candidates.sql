WITH instant AS MATERIALIZED (
  SELECT statement_timestamp() AS read_at
), eligible AS MATERIALIZED (
  SELECT z.id, s.last_runner_id
  FROM fleet.fleets z
  CROSS JOIN instant
  LEFT JOIN runner.fleet_slots s ON s.fleet_id = z.id
  WHERE z.status = $1
    AND EXISTS (SELECT 1 FROM fleet.admissions WHERE settled_at IS NULL)
    AND (s.lease_expires_at IS NULL OR s.lease_expires_at < instant.read_at)
    AND (s.hold_expires_at IS NULL
         OR s.hold_expires_at <= instant.read_at
         OR s.last_runner_id = $2::uuid
         OR NOT EXISTS (
           SELECT 1 FROM runner.runners r
           WHERE r.id = s.last_runner_id
             AND r.last_seen_at > instant.read_at - $4::interval
             AND r.status = $5 AND r.degraded_reason IS NULL))
    AND z.required_tags <@ (
      SELECT COALESCE(array_agg(e), '{}'::text[])
      FROM jsonb_array_elements_text(
        (SELECT CASE WHEN jsonb_typeof(labels) = 'array' THEN labels ELSE '[]'::jsonb END
         FROM runner.runners WHERE id = $2::uuid)) AS e)
)
SELECT z.id::text
FROM eligible z
JOIN LATERAL (
  SELECT a.parked_at FROM fleet.admissions a
  WHERE a.fleet_id = z.id AND a.settled_at IS NULL
  ORDER BY a.created_at, a.sequence LIMIT 1
) head ON head.parked_at IS NULL
ORDER BY z.id = ANY((SELECT ($6::text[])::uuid[])::uuid[]) DESC,
         (z.id = $7::uuid) DESC NULLS LAST,
         (z.last_runner_id = $2::uuid) DESC NULLS LAST,
         -- Only each fleet's head is ordered. Spread runners across fleets
         -- rather than making all of them contend for the same oldest fleet.
         random()
LIMIT $3
