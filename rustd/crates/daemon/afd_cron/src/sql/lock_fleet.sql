SELECT f.id
  FROM fleet.fleets f
 WHERE f.id = $1
   FOR UPDATE OF f
