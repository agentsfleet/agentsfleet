SELECT count(*)
  FROM (SELECT l.fleet_id
          FROM runner.leases l
          JOIN runner.runners r ON r.id = l.runner_id
         WHERE r.host_id LIKE $1 AND l.status = $2
         GROUP BY l.fleet_id
        HAVING count(*) > 1) doubled
