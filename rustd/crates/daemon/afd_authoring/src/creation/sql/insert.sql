INSERT INTO fleet.fleets
    (id, workspace_id, tenant_id, name, status, head_revision_id, required_tags,
     setup_stream_ready, initial_start, memory_reads_workspace, memory_publishes_workspace,
     resource_version, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6::uuid, $7::text[],
        false, $8::jsonb, false, false,
        $9, now(), now())
