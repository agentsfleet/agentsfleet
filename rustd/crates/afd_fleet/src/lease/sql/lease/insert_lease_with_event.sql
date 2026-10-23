WITH held AS (
  UPDATE runner.fleet_slots
  SET updated_at = now()
  WHERE fleet_id = $3::uuid AND fencing_token = $10 AND lease_expires_at > $11::timestamptz
  RETURNING fleet_id, lease_expires_at
), inserted AS (
  INSERT INTO runner.leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, fleet_event_id, event_created_at,
   posture, provider, model, last_metered_at,
   fencing_token, lease_expires_at, status, posted_message_count, created_at, updated_at)
  SELECT $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, e.id, $20::timestamptz,
         $7, $8, $9, $11::timestamptz,
         $10, h.lease_expires_at, $12, 0, now(), now()
  FROM held h
  JOIN fleet.events e ON e.fleet_id = $3::uuid AND e.event_id = $6
  -- The scope proof runner.leases holds no foreign key for: a workspace or
  -- tenant that is not the fleet's writes no lease, so the caller issues none.
  JOIN fleet.fleets f ON f.id = $3::uuid AND f.workspace_id = $4::uuid AND f.tenant_id = $5::uuid
  RETURNING id, runner_id, fleet_id, fleet_event_id
), audit AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $13::uuid, i.runner_id, $14::text,
         jsonb_build_object($15::text, i.id::text, $16::text, i.fleet_id::text,
                            $17::text, $6::text, $18::text, $19::text),
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
