WITH held AS (
  UPDATE runner.fleet_slots
  SET metered_input_token_count  = CASE WHEN $15::boolean THEN 0 ELSE metered_input_token_count END,
      metered_cached_token_count = CASE WHEN $15::boolean THEN 0 ELSE metered_cached_token_count END,
      metered_output_token_count = CASE WHEN $15::boolean THEN 0 ELSE metered_output_token_count END,
      last_metered_at = CASE WHEN $15::boolean THEN $12::timestamptz ELSE last_metered_at END,
      updated_at      = CASE WHEN $15::boolean THEN now() ELSE updated_at END
  WHERE fleet_id = $3::uuid AND fencing_token = $11 AND lease_expires_at > $12::timestamptz
  RETURNING fleet_id, lease_expires_at
), inserted AS (
  INSERT INTO runner.leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, fleet_event_id, event_created_at,
   receipt, posture, provider, model,
   metered_input_token_count, metered_cached_token_count, metered_output_token_count,
   fencing_token, lease_expires_at, status, posted_message_count, created_at, updated_at)
  SELECT $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, e.id, $22::timestamptz,
         $7, $8, $9, $10,
         0, 0, 0,
         $11, h.lease_expires_at, $13, 0, now(), now()
  FROM held h
  JOIN fleet.events e ON e.fleet_id = $3::uuid AND e.event_id = $6
  RETURNING id, runner_id, fleet_id, fleet_event_id
), audit AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $14::uuid, i.runner_id, $16::text,
         jsonb_build_object($17::text, i.id::text, $18::text, i.fleet_id::text,
                            $19::text, $6::text, $20::text, $21::text),
         NULL, now()
  FROM inserted i
  RETURNING id
)
INSERT INTO runner.lifetime_counters AS c
  (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
   created_at, updated_at)
SELECT runner_id, 1, 0, 0, 0, now(), now()
FROM inserted
ON CONFLICT (runner_id) DO UPDATE
   SET acquired_count = c.acquired_count + 1, updated_at = now()
