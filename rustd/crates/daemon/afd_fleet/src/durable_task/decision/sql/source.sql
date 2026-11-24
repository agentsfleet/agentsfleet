-- The reservation's read of its source turn: `../../sql/lock_source.sql` with no lock. The
-- issue statement locks the turn and re-checks that it is unsettled and, when it is not the run's own,
-- still assigned to this run and fence.
SELECT actor, producer, request_json::text AS request_json, reply_provider, reply_address,
       connector.slack_person_allowed(workspace_id, actor, $4, $5, $6) AS slack_person_allowed,
       input_run_id, input_fence
FROM thread.turns
WHERE id = $1 AND thread_id = $2 AND workspace_id = $3
  AND settled_at IS NULL
