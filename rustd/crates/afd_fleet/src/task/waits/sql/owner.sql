SELECT w.id::text, w.task_id::text, w.fleet_id::text, w.workspace_id::text,
       w.task_generation, w.stop_generation, w.installation_id, w.repository_id,
       w.pull_request_number, w.expected_head, w.checks, w.baseline_digest,
       w.fallback_count, w.initial_read_at, w.signalled_at, w.signal_sequence, w.observed_signal_sequence,
       w.claim_id::text, w.claim_expires_at > statement_timestamp() AS claim_valid, w.expires_at,
       t.status, t.generation AS live_generation, t.stop_generation AS live_stop_generation,
       t.accepted_revision_id::text, t.target, t.used_fallback_reads, t.max_fallback_reads,
       t.reply_provider, t.reply_address, i.admission_id::text AS source_admission_id,
       t.expires_at > statement_timestamp() AND t.used_model_nanos < t.max_model_nanos - t.reserved_model_nanos
         AND t.used_running_nanos < t.max_running_nanos AND t.used_leases < t.max_leases AS within_limits,
       w.due_at <= statement_timestamp() AND w.cooldown_until <= statement_timestamp() AS due
FROM fleet.task_waits w JOIN fleet.tasks t ON t.id = w.task_id AND t.fleet_id = w.fleet_id AND t.workspace_id = w.workspace_id
JOIN fleet.task_items i ON i.id = t.originating_task_item_id
WHERE w.id = $1::uuid AND w.consumed_at IS NULL AND w.cancelled_at IS NULL
FOR UPDATE OF w SKIP LOCKED
