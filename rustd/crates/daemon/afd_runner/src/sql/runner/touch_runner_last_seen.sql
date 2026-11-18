-- A failed transition write can prove liveness, never continuous healthy return.
WITH locked AS MATERIALIZED (
  SELECT id, token_hash, connection_id, status FROM runner.runners WHERE id = $1 FOR UPDATE
), observed AS MATERIALIZED (
  SELECT r.id, clock_timestamp() AS at FROM locked r
  WHERE $2::text IS NULL OR (r.token_hash = $2 AND r.connection_id IS NOT DISTINCT FROM $3::uuid AND r.status = $4)
)
UPDATE runner.runners r SET last_seen_at = o.at, cordon_healthy_since_at = NULL, updated_at = o.at
FROM observed o WHERE r.id = o.id RETURNING r.id
