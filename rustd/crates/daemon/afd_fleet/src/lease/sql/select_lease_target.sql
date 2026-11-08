SELECT l.fleet_id, l.event_id, l.created_at, l.event_created_at,
       l.status, l.lease_expires_at, l.fencing_token,
       l.status = $3 AND l.lease_expires_at > statement_timestamp()
           AND EXISTS (SELECT 1 FROM runner.fleet_slots s WHERE s.fleet_id = l.fleet_id
               AND s.last_runner_id = l.runner_id AND s.fencing_token = l.fencing_token
               AND s.lease_expires_at > statement_timestamp()) AS live
FROM runner.leases l
JOIN fleet.events e ON e.event_id = l.event_id
WHERE l.id = $1 AND l.runner_id = $2
