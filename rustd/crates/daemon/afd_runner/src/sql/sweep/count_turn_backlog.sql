SELECT count(*) AS rows,
       COALESCE(GREATEST(0, FLOOR(EXTRACT(EPOCH FROM
         (statement_timestamp() - min(created_at)))))::bigint, 0) AS oldest_seconds
FROM thread.turns
-- Admitted fleet turns: a setup draft's turn has no producer.
WHERE settled_at IS NULL AND producer IS NOT NULL
