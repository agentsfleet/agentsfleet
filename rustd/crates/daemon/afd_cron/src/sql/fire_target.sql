SELECT s.fleet_id, f.workspace_id, s.message, s.cron_expression, s.timezone, s.once,
       s.desired_status, f.status, s.fire_at AS scheduled_at
  FROM fleet.schedules s
  JOIN fleet.fleets f ON f.id = s.fleet_id
 WHERE s.id = $1
