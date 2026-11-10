-- The caller holds the host row (`lock_host.sql`) through this transaction, so
-- this statement's snapshot, taken after that lock, sees every earlier
-- claimant's commit: the census below counts them (M245 §5). A full host, a
-- moved slot fence or a moved placement choice writes nothing.
WITH instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at
), host AS MATERIALIZED (
  SELECT r.id
  FROM runner.runners r CROSS JOIN instant i
  WHERE r.id = $2
    AND (SELECT count(*) FROM runner.leases l
          WHERE l.runner_id = $2 AND l.status = $11
            AND l.lease_expires_at > i.checked_at)
      + (SELECT count(*) FROM authoring.attempts a
          WHERE a.runner_id = $2 AND a.status = $11
            AND a.lease_expires_at > i.checked_at)
      < r.worker_count
), held AS (
  UPDATE runner.fleet_slots
  SET updated_at = now()
  WHERE fleet_id = $3 AND fencing_token = $10
    AND lease_expires_at > (SELECT checked_at FROM instant)
    -- The deadline the claim answered, which a pass that outlived it misses.
    AND $19 > (SELECT checked_at FROM instant)
    AND EXISTS (SELECT 1 FROM host)
  RETURNING fleet_id, lease_expires_at
), inserted AS (
  INSERT INTO runner.leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, event_created_at,
   posture, provider, model, last_metered_at,
   fencing_token, lease_expires_at, status, posted_message_count, decision_snapshot, created_at, updated_at)
  SELECT $1, $2, $3, $4, $5, e.event_id, $15,
         $7, $8, $9, i.checked_at,
         $10, h.lease_expires_at, $11, 0, $16, i.checked_at, i.checked_at
  FROM held h
  CROSS JOIN instant i
  JOIN fleet.events e ON e.fleet_id = $3 AND e.event_id = $6
  -- The scope proof runner.leases holds no foreign key for: a workspace or
  -- tenant that is not the fleet's writes no lease, so the caller issues none.
  JOIN fleet.fleets f ON f.id = $3 AND f.workspace_id = $4 AND f.tenant_id = $5
  -- Recheck after held acquired the slot lock and after profile lookup.
  WHERE h.lease_expires_at > clock_timestamp()
  RETURNING id, runner_id, fleet_id, event_id, created_at
), chosen AS (
  -- The placement choice the host lock read: the next offer prefers the other kind.
  UPDATE runner.runners
  SET lease_version = lease_version + 1, last_lease_type = $18::text, updated_at = now()
  WHERE id = $2 AND lease_version = $17::bigint AND EXISTS (SELECT 1 FROM inserted)
  RETURNING id
), audit AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $12, i.runner_id, $13, $14,
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
-- No row when the choice moved: the caller rolls the lease back with it.
SELECT created_at FROM inserted
WHERE EXISTS (SELECT 1 FROM tallied) AND EXISTS (SELECT 1 FROM chosen)
