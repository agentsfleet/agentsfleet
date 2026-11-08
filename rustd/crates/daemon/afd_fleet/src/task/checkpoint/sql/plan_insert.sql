WITH counter AS (
UPDATE fleet.tasks SET checkpoint_sequence = COALESCE(checkpoint_sequence, 0) + 1,
    checkpoint_cleanup_at = LEAST(expires_at, clock_timestamp() + make_interval(secs => $23)), updated_at = now()
WHERE id = $2::uuid RETURNING id, checkpoint_sequence, expires_at
), checkpoint AS (
INSERT INTO fleet.task_checkpoints (id, task_id, fleet_id, workspace_id, generation, task_generation,
    accepted_revision_id, stop_generation, mode, repository, task_base_id,
    commit_oid, format_version, toolbox_digest, page_count, page_ids, upload_budget_bytes, uploaded_bytes,
    logical_bytes, entry_count, covered_input_sequence, effect_watermark, owner_lease_id, owner_fence,
    expires_at, transfer_expires_at, manifest_root, ready_at, created_at, updated_at)
SELECT $1::uuid, t.id, $3::uuid, $4::uuid,
    t.checkpoint_sequence,
    $5, $6::uuid, $7, $10, $11, $12::uuid, $13, $14, $15, $16, $17::text[]::uuid[], $18, 0,
    $19, $20, 0, 0, $21::uuid, $22,
    LEAST(t.expires_at, clock_timestamp() + make_interval(secs => $23)),
    LEAST(t.expires_at, clock_timestamp() + make_interval(secs => $23)),
    $24, CASE WHEN $24::text IS NOT NULL THEN clock_timestamp() END, now(), now()
FROM counter t
RETURNING id
)
INSERT INTO fleet.task_checkpoint_operations (task_checkpoint_id, task_id, fleet_id, workspace_id, operation_id, request_digest, created_at)
SELECT id, $2::uuid, $3::uuid, $4::uuid, $8::uuid, $9, now() FROM checkpoint
