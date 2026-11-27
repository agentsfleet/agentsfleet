WITH counter AS (
UPDATE fleet.durable_tasks SET checkpoint_sequence = COALESCE(checkpoint_sequence, 0) + 1,
    checkpoint_cleanup_at = LEAST(expires_at, clock_timestamp() + make_interval(secs => $21)), updated_at = now()
WHERE id = $2 RETURNING id, checkpoint_sequence, expires_at
), checkpoint AS (
INSERT INTO runner.checkpoints (id, durable_task_id, fleet_id, workspace_id, generation, durable_task_generation,
    stop_generation, mode, repository, checkpoint_base_id,
    commit_oid, format_version, toolbox_digest, page_ids, upload_budget_bytes, uploaded_bytes,
    logical_bytes, entry_count, covered_input_sequence, effect_watermark, owner_run_id, owner_fence,
    expires_at, manifest_root, ready_at, cipher, parent_checkpoint_id, created_at, updated_at)
SELECT $1, t.id, $3, $4,
    t.checkpoint_sequence,
    $5, $6, $9, $10, $11, $12, $13, $14, $15, $16, 0,
    $17, $18, 0, 0, $19, $20,
    LEAST(t.expires_at, clock_timestamp() + make_interval(secs => $21)),
    $22, CASE WHEN $22::text IS NOT NULL THEN clock_timestamp() END, $24, $25, now(), now()
FROM counter t
RETURNING id
)
INSERT INTO thread.replay_receipts (scope, operation_id, request_digest, resource_id, created_at)
SELECT $23, $7, $8, id, now() FROM checkpoint
