WITH inserted AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  VALUES ($1::uuid, $2::uuid, $3::text,
          jsonb_build_object($4::text,
            regexp_replace(to_char($5::timestamptz AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS'),
                           '\.?0+$', '') || 'Z'),
          $5::timestamptz, now())
  ON CONFLICT (runner_id, last_seen_at) WHERE last_seen_at IS NOT NULL
  DO NOTHING
  RETURNING 1
)
SELECT COUNT(*)::bigint FROM inserted
