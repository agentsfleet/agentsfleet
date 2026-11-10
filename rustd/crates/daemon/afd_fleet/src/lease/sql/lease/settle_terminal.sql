UPDATE fleet.admissions a
SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now()
WHERE a.fleet_id = $1 AND a.id = $2 AND a.lease_fence = $3
  AND a.settled_at IS NULL
  AND EXISTS (SELECT 1 FROM fleet.events e WHERE e.fleet_id = a.fleet_id
    AND e.event_id = a.id AND e.status != $4)
