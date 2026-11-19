SELECT id,
       workspace_id,
       tenant_id,
       user_id,
       draft_id,
       message_sequence,
       fencing_token,
       model_snapshot::text AS model_snapshot,
       status,
       run_expires_at,
       expires_at,
       last_activity_sequence,
       model_token_hash,
       call_count,
       output_token_count,
       usage_unknown,
       reserved_nanos,
       created_at,
       reported_runtime_ms,
       failure_reason,
       max_model_calls, max_wall_seconds, max_proposal_corrections, provider_timeout_seconds
  FROM authoring.attempts
 WHERE run_id = $1 AND runner_id = $2
   FOR UPDATE
