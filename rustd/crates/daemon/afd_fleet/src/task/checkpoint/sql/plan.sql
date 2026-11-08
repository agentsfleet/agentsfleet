SELECT c.id::text, c.task_id::text, c.workspace_id::text, c.fleet_id::text,
       c.generation, c.task_generation, c.stop_generation,
       r.request_digest, c.mode, c.repository, c.task_base_id::text, c.format_version,
       c.toolbox_digest, c.page_count, c.page_ids::text[], c.upload_budget_bytes,
       c.uploaded_bytes, c.logical_bytes, c.entry_count, c.manifest_root, c.history_digest,
       c.covered_input_sequence, c.effect_watermark, c.owner_lease_id::text, c.owner_fence,
       c.expires_at, c.transfer_expires_at, c.ready_at IS NOT NULL AS ready,
       c.abandoned_at IS NOT NULL AS abandoned,
       COALESCE(b.page_ids::text[], ARRAY[]::text[]) AS base_page_ids,
       b.manifest_root AS base_manifest_root, b.commit_oid AS base_commit_oid
FROM fleet.task_checkpoints c
JOIN fleet.task_checkpoint_operations r ON r.task_checkpoint_id = c.id AND r.task_id = c.task_id
LEFT JOIN fleet.task_bases b ON b.id = c.task_base_id AND b.workspace_id = c.workspace_id AND b.repository = c.repository
WHERE c.id = $1::uuid AND c.task_id = $2::uuid
FOR UPDATE OF c
