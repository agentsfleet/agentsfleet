SELECT count(*)
  FROM fleet.schedules
 WHERE fleet_id = $1::uuid AND recipe IS NULL
