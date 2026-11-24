-- Fleets this run's chat tenants created from a first message, holding more than one active run.
SELECT count(*)
  FROM (SELECT l.fleet_id
          FROM thread.runs l
          JOIN fleet.fleets f ON f.id = l.fleet_id
         WHERE f.tenant_id = ANY($1) AND l.status = $2
         GROUP BY l.fleet_id
        HAVING count(*) > 1) doubled
