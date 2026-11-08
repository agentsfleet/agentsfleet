SELECT status, COUNT(*)::bigint AS fleets
FROM fleet.fleets
GROUP BY status
