SELECT count(*)
  FROM runner.runners r
 WHERE r.host_id LIKE $1
   AND (SELECT count(*) FROM thread.runs l WHERE l.runner_id = r.id AND l.status = $2)
     > r.worker_count
