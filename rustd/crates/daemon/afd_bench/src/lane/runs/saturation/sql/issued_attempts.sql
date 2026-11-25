-- Runs the window issued for this run's chat tenants: each is a run of a fleet a first message made.
SELECT count(*)
  FROM thread.runs l
  JOIN fleet.fleets f ON f.id = l.fleet_id
 WHERE f.tenant_id = ANY($1)
