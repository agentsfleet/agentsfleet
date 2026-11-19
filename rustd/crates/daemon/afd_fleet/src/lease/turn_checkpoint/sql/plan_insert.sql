INSERT INTO runner.turn_checkpoints
    (id, event_id, fleet_id, workspace_id, setup_revision_id, generation,
     expected_turn_checkpoint_id, owner_lease_id, owner_fence, format_version,
     has_workspace, toolbox_digest, page_count, page_ids, upload_budget_bytes,
     uploaded_bytes, logical_bytes, entry_count, history_digest, expires_at,
     created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
        0, $16, $17, $18, clock_timestamp() + make_interval(secs => $19),
        now(), now())
