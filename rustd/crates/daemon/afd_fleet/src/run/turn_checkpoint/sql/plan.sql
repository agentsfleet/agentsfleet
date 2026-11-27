SELECT id, owner_run_id, owner_fence, generation, format_version, cipher,
       expected_head_checkpoint_id AS expected_head_id, parent_checkpoint_id AS parent_id, toolbox_digest IS NOT NULL AS has_workspace,
       toolbox_digest, page_ids, history_digest,
       manifest_root, covered_fold_ids, covered_fold_sequences,
       expires_at, abandoned_at IS NOT NULL AS abandoned, ready_at IS NOT NULL AS ready,
       upload_budget_bytes, uploaded_bytes, logical_bytes, entry_count
FROM runner.checkpoints
WHERE id = $1 AND turn_id = $2 AND fleet_id = $3 AND workspace_id = $4
FOR UPDATE
