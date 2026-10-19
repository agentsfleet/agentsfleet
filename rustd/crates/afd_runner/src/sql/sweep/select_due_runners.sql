SELECT r.id::text AS id, r.last_seen_at, r.status, statement_timestamp() AS checked_at
FROM runner.runners r
WHERE (r.last_seen_at <> $1::timestamptz
       AND r.last_seen_at < statement_timestamp() - $2::interval)
   OR r.status = $5::text
   OR (r.status <> $3::text AND EXISTS (
        SELECT 1 FROM runner.leases l
        WHERE l.runner_id = r.id AND l.status = $4::text
      ))
ORDER BY r.updated_at ASC, r.id ASC
LIMIT $6::bigint
