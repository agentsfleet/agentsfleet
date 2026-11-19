SELECT id, setup_revision_id, owner_run_id, owner_fence, generation, format_version,
       expected_turn_checkpoint_id AS expected_head_id, has_workspace, toolbox_digest, page_ids, history_digest,
       manifest_root, covered_fold_ids, covered_fold_sequences,
       expires_at, abandoned_at IS NOT NULL AS abandoned, ready_at IS NOT NULL AS ready,
       upload_budget_bytes, uploaded_bytes, logical_bytes, entry_count
FROM runner.turn_checkpoints
WHERE id = $1 AND event_id = $2 AND fleet_id = $3 AND workspace_id = $4
FOR UPDATE
