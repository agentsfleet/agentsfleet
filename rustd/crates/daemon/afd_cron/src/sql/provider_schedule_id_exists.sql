SELECT 1::bigint
  FROM fleet.schedules
 WHERE fleet_id = $1 AND provider_schedule_id = $2
 LIMIT 1
