-- Authority and the health witness are rechecked after the host lock wait.
WITH locked AS MATERIALIZED (
  SELECT id, last_seen_at, cordon_reason, cordoned_at, cordon_healthy_since_at,
         degraded_reason, status, token_hash, connection_id,
         jsonb_build_array(sandbox_tier, network_policy, registry_allowlist, worker_count, sandbox_config, durable_tasks_enabled, decision_platform_model_id) AS policy_witness
  FROM runner.runners WHERE id = $1 FOR UPDATE
), observed AS MATERIALIZED (
  SELECT r.*, clock_timestamp() AS checked_at FROM locked r
  WHERE $7::text IS NULL OR (r.token_hash = $7 AND r.connection_id IS NOT DISTINCT FROM $8::uuid AND r.status = $16)
), proof AS MATERIALIZED (
  SELECT r.*, ($9::boolean AND r.policy_witness = $17::jsonb
               AND r.degraded_reason IS NULL AND r.status = $16 AND r.cordon_reason = $15) AS healthy
  FROM observed r
), lifting AS MATERIALIZED (
  SELECT p.*, COALESCE(p.healthy AND p.last_seen_at > p.checked_at - $10::interval
       AND p.cordon_healthy_since_at <= p.checked_at - $11::interval, false) AS lift
  FROM proof p
), bumped AS (
  UPDATE runner.runners r SET last_seen_at = p.checked_at, updated_at = p.checked_at,
    cordon_reason = CASE WHEN p.lift THEN NULL ELSE r.cordon_reason END,
    cordoned_at = CASE WHEN p.lift THEN NULL ELSE r.cordoned_at END,
    cordon_healthy_since_at = CASE WHEN p.lift OR NOT COALESCE(p.healthy, false) THEN NULL
      WHEN p.cordon_healthy_since_at IS NULL OR p.last_seen_at <= p.checked_at - $10::interval THEN p.checked_at
      ELSE p.cordon_healthy_since_at END
  FROM lifting p WHERE r.id = p.id
  RETURNING r.id, p.last_seen_at AS previous, p.checked_at, p.lift
), online AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $2, id, $3, jsonb_build_object($4::text, regexp_replace(to_char(previous AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS'), '\.?0+$', '') || 'Z'), NULL, checked_at FROM bumped
  WHERE previous = $5::timestamptz OR previous < checked_at - $6::interval
  RETURNING id
), lifted AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $12, id, $13, jsonb_build_object($14::text, $15::text), NULL, checked_at FROM bumped WHERE lift
  RETURNING id
)
SELECT EXISTS(SELECT 1 FROM bumped) AS accepted, EXISTS(SELECT 1 FROM lifted) AS lifted
