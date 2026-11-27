INSERT INTO runner.checkpoints
    (id, turn_id, fleet_id, workspace_id, generation,
     expected_head_checkpoint_id, owner_run_id, owner_fence, format_version,
     toolbox_digest, page_ids, upload_budget_bytes,
     uploaded_bytes, logical_bytes, entry_count, history_digest, expires_at,
     cipher, parent_checkpoint_id, engine, guest_agent_digest, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
        0, $13, $14, $15, clock_timestamp() + make_interval(secs => $16),
        $17, $18, $19, $20, now(), now())
