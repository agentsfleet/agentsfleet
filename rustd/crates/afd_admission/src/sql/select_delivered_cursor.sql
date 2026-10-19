SELECT a.receipt
FROM fleet.admissions a
JOIN fleet.events e
  ON e.fleet_id = a.fleet_id
 AND e.event_id = a.id::text
WHERE a.fleet_id = $1::uuid AND a.receipt IS NOT NULL
ORDER BY split_part(a.receipt, '-', 1)::bigint DESC,
         split_part(a.receipt, '-', 2)::bigint DESC
LIMIT 1
