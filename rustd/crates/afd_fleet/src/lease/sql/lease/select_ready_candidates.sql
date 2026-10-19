WITH instant AS MATERIALIZED (
  SELECT statement_timestamp() AS read_at
)
SELECT z.id::text
FROM fleet.fleets z
CROSS JOIN instant
LEFT JOIN runner.fleet_slots a ON a.fleet_id = z.id
WHERE z.status = $1
  AND z.id = ANY(($3::text[])::uuid[])
  AND (a.lease_expires_at IS NULL OR a.lease_expires_at < instant.read_at)
  AND (a.hold_expires_at IS NULL
       OR a.hold_expires_at <= instant.read_at
       OR a.last_runner_id = $2::uuid
       OR NOT EXISTS (
            SELECT 1 FROM runner.runners r
            WHERE r.id = a.last_runner_id
              AND r.last_seen_at > instant.read_at - $5::interval
              AND r.status = $6 AND r.degraded_reason IS NULL))
  AND z.required_tags <@ (
        SELECT COALESCE(array_agg(e), '{}'::text[])
        FROM jsonb_array_elements_text(
               (SELECT CASE WHEN jsonb_typeof(labels) = 'array'
                            THEN labels ELSE '[]'::jsonb END
                FROM runner.runners WHERE id = $2::uuid)
             ) AS e
      )
ORDER BY z.id = ANY((SELECT ($7::text[])::uuid[])::uuid[]) DESC,
         (a.last_runner_id = $2::uuid) DESC NULLS LAST, random()
LIMIT $4
