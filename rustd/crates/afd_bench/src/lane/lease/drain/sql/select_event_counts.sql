SELECT count(*), count(*) FILTER (WHERE status = $2)
FROM fleet.events
WHERE fleet_id = ANY($1::uuid[])
