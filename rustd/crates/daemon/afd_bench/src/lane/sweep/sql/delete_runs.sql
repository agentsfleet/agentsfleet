-- A run's runners' runs, by the prefix in the runner's host id. A runner's deletion keeps its
-- runs as history naming no runner, so the sweep removes them first, while they still name it.
DELETE FROM thread.runs l
USING runner.runners r
WHERE l.runner_id = r.id AND r.host_id LIKE $1
