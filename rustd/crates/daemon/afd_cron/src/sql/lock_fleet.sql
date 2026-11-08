SELECT f.id::text
  FROM fleet.fleets f
 WHERE f.id = $1::uuid
   FOR UPDATE OF f
