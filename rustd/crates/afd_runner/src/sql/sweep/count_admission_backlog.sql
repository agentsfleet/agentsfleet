SELECT count(*) AS rows,
       COALESCE(GREATEST(0, FLOOR(EXTRACT(EPOCH FROM
         (statement_timestamp() - min(created_at)))))::bigint, 0) AS oldest_seconds
FROM fleet.admissions
WHERE settled_at IS NULL
