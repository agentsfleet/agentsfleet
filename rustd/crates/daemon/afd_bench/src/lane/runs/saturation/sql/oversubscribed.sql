SELECT count(*)
  FROM runner.runners r
 WHERE r.host_id LIKE $1
   AND (SELECT count(*) FROM thread.runs l WHERE l.runner_id = r.id AND l.status = $2)
     -- Setup turns the authoring path still runs; this term goes with setup.attempts.
     + (SELECT count(*) FROM setup.attempts a WHERE a.runner_id = r.id AND a.status = $3)
     > r.worker_count
