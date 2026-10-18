SELECT s.fleet_id::text, f.workspace_id::text, s.message, s.once,
       s.desired_status, f.status
  FROM fleet.schedules s
  JOIN fleet.fleets f ON f.id = s.fleet_id
 WHERE s.id = $1::uuid
