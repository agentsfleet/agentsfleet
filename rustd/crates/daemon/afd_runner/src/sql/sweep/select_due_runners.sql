SELECT r.id, r.last_seen_at, r.status, statement_timestamp() AS checked_at
FROM runner.runners r
WHERE (r.last_seen_at <> $1::timestamptz
       AND r.last_seen_at < statement_timestamp() - $2::interval
       AND (NOT EXISTS (SELECT 1 FROM runner.events e
              WHERE e.runner_id = r.id AND e.last_seen_at = r.last_seen_at)
            OR (r.status = $3::text AND r.cordon_reason IS NULL)))
   OR (r.status = $5::text
       AND NOT EXISTS (SELECT 1 FROM runner.leases l WHERE l.runner_id = r.id
           AND l.status = $4::text AND l.lease_expires_at > statement_timestamp())
       AND NOT EXISTS (SELECT 1 FROM authoring.attempts a WHERE a.runner_id = r.id
           AND a.status = $4::text AND a.lease_expires_at > statement_timestamp()))
ORDER BY r.updated_at, r.id LIMIT $6::bigint
