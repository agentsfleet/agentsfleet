SELECT count(*)
  FROM runner.runners r
 WHERE r.host_id LIKE $1
   AND (SELECT count(*) FROM runner.leases l WHERE l.runner_id = r.id AND l.status = $2)
     + (SELECT count(*) FROM authoring.attempts a WHERE a.runner_id = r.id AND a.status = $3)
     > r.worker_count
