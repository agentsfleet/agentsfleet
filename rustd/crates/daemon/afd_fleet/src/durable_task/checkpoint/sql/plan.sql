SELECT c.id, c.durable_task_id, c.workspace_id, c.fleet_id,
       c.generation, c.durable_task_generation, c.stop_generation,
       r.request_digest, c.mode, c.repository, c.durable_task_base_id, c.format_version,
       c.toolbox_digest, c.page_count, c.page_ids, c.upload_budget_bytes,
       c.uploaded_bytes, c.logical_bytes, c.entry_count, c.manifest_root, c.history_digest,
       c.covered_input_sequence, c.effect_watermark, c.owner_run_id, c.owner_fence,
       c.expires_at, c.transfer_expires_at, c.ready_at IS NOT NULL AS ready,
       c.abandoned_at IS NOT NULL AS abandoned,
       COALESCE(b.page_ids, '{}') AS base_page_ids,
       b.manifest_root AS base_manifest_root, b.commit_oid AS base_commit_oid
FROM fleet.durable_task_checkpoints c
JOIN fleet.durable_task_checkpoint_operations r ON r.durable_task_checkpoint_id = c.id AND r.durable_task_id = c.durable_task_id
LEFT JOIN fleet.durable_task_bases b ON b.id = c.durable_task_base_id AND b.workspace_id = c.workspace_id AND b.repository = c.repository
WHERE c.id = $1 AND c.durable_task_id = $2
FOR UPDATE OF c
