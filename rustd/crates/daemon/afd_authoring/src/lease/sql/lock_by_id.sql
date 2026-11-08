SELECT id,
       workspace_id,
       tenant_id,
       user_id,
       draft_id,
       message_sequence,
       fencing_token,
       model_snapshot::text AS model_snapshot,
       status,
       lease_expires_at,
       expires_at,
       last_activity_sequence,
       model_token_hash,
       call_count,
       output_token_count,
       usage_unknown,
       reserved_nanos,
       created_at,
       reported_runtime_ms,
       failure_reason
  FROM authoring.attempts
 WHERE id = $1
   FOR UPDATE
