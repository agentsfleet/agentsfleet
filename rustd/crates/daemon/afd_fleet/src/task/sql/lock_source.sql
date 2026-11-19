SELECT actor, producer, request_json::text AS request_json, reply_provider, reply_address,
       connector.slack_person_allowed(workspace_id, actor, $4, $5, $6) AS slack_person_allowed,
       input_claim_run_id, input_claim_fence
FROM thread.turns
WHERE id = $1 AND thread_id = $2 AND workspace_id = $3
  AND settled_at IS NULL
FOR UPDATE
