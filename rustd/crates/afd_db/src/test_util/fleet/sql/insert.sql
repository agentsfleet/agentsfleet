INSERT INTO fleet.fleets (id, workspace_id, tenant_id, name, status, required_tags, memory_reads_workspace, memory_publishes_workspace, resource_version, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6::text[], $7, $8, $9, coalesce($10::timestamptz, now()), coalesce($11::timestamptz, $10::timestamptz, now()))
