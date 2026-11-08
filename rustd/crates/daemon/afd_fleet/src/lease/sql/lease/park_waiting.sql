UPDATE fleet.admissions a SET parked_at = clock_timestamp(), updated_at = now()
WHERE a.fleet_id = $1::uuid AND a.id = $2::uuid AND a.lease_fence = $3
  AND a.settled_at IS NULL
  AND EXISTS (SELECT 1 FROM fleet.approval_gates g
    WHERE g.fleet_id = a.fleet_id AND g.resolved_at IS NULL
      AND (g.event_id IS NULL OR g.event_id = a.id::text))
