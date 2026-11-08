SELECT count(*)
  FROM fleet.schedules
 WHERE fleet_id = $1 AND recipe IS NULL
