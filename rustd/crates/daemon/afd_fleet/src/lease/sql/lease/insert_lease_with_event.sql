WITH held AS (
  UPDATE runner.fleet_slots
  SET updated_at = now()
  WHERE fleet_id = $3 AND fencing_token = $10 AND lease_expires_at > $11
  RETURNING fleet_id, lease_expires_at
), inserted AS (
  INSERT INTO runner.leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, event_created_at,
   posture, provider, model, last_metered_at,
   fencing_token, lease_expires_at, status, posted_message_count, decision_snapshot, created_at, updated_at)
  SELECT $1, $2, $3, $4, $5, e.event_id, $16,
         $7, $8, $9, $11,
         $10, h.lease_expires_at, $12, 0, $17, $11, $11
  FROM held h
  JOIN fleet.events e ON e.fleet_id = $3 AND e.event_id = $6
  -- The scope proof runner.leases holds no foreign key for: a workspace or
  -- tenant that is not the fleet's writes no lease, so the caller issues none.
  JOIN fleet.fleets f ON f.id = $3 AND f.workspace_id = $4 AND f.tenant_id = $5
  -- Recheck after held acquired the slot lock and after profile lookup.
  WHERE h.lease_expires_at > clock_timestamp()
  RETURNING id, runner_id, fleet_id, event_id, created_at
), audit AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $13, i.runner_id, $14, $15,
         NULL, now()
  FROM inserted i
  RETURNING id
), tallied AS (
INSERT INTO runner.lifetime_counters AS c
  (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
   created_at, updated_at)
SELECT runner_id, 1, 0, 0, 0, now(), now()
FROM inserted
ON CONFLICT (runner_id) DO UPDATE
   SET acquired_count = c.acquired_count + 1, updated_at = now()
RETURNING runner_id
)
-- Use the value stored by TIMESTAMPTZ(3), not a separately normalized cache timestamp.
SELECT created_at FROM inserted WHERE EXISTS (SELECT 1 FROM tallied)
