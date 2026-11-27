SELECT c.id, c.durable_task_id, c.workspace_id,
       c.generation, c.durable_task_generation, c.stop_generation,
       c.mode, c.repository, c.checkpoint_base_id, c.parent_checkpoint_id, c.format_version, c.cipher,
       c.toolbox_digest, c.engine, c.guest_agent_digest, cardinality(c.page_ids) AS page_count, c.page_ids, c.upload_budget_bytes,
       c.uploaded_bytes, c.logical_bytes, c.entry_count, c.manifest_root, c.history_digest,
       c.covered_input_sequence, c.effect_watermark, c.owner_run_id, c.owner_fence,
       c.expires_at, c.ready_at IS NOT NULL AS ready,
       c.abandoned_at IS NOT NULL AS abandoned,
       COALESCE(b.page_ids, '{}') AS base_page_ids,
       b.manifest_root AS base_manifest_root, b.commit_oid AS base_commit_oid
FROM runner.checkpoints c
LEFT JOIN runner.checkpoint_bases b ON b.id = c.checkpoint_base_id AND b.workspace_id = c.workspace_id AND b.repository = c.repository
WHERE c.id = $1 AND c.durable_task_id = $2
FOR UPDATE OF c
