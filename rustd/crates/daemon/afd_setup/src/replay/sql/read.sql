-- A create's receipt inside the replay window, and whether the fleet it names still exists:
-- the receipt has no foreign key, so it outlives the fleet it created.
SELECT r.request_digest, r.resource_id AS resource_id,
       EXISTS (SELECT 1 FROM fleet.fleets f WHERE f.id = r.resource_id) AS live
  FROM thread.replay_receipts r
 WHERE r.scope = $1 AND r.operation_id = $2
   AND r.created_at > statement_timestamp() - $3::interval
