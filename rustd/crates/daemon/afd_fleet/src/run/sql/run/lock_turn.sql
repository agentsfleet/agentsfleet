WITH slot AS MATERIALIZED (
  SELECT s.fleet_id FROM thread.threads s
  WHERE s.fleet_id = $1 AND s.fencing_token = $3
  FOR NO KEY UPDATE OF s
), admission AS (
  SELECT a.id FROM thread.turns a JOIN slot s ON s.fleet_id = a.thread_id
  WHERE a.id = $2 AND a.run_fence = $3 AND a.settled_at IS NULL
  FOR UPDATE OF a
)
SELECT EXISTS (SELECT 1 FROM admission)
