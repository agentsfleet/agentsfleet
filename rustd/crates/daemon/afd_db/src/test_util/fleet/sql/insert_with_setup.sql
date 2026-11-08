WITH fleet AS (
    INSERT INTO fleet.fleets (id, workspace_id, tenant_id, name, status, required_tags, memory_reads_workspace, memory_publishes_workspace, resource_version, created_at, updated_at, head_revision_id)
    VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6::text[], $7, $8, $9, coalesce($10::timestamptz, now()), coalesce($11::timestamptz, $10::timestamptz, now()), $12::uuid)
    RETURNING id, created_at
)
INSERT INTO fleet.revisions (id, fleet_id, revision, definition, canonical_digest, policy_semantics, runtime_model_ref, created_at)
SELECT $12::uuid, id, $13, $14::jsonb, $15, $16, $17, created_at FROM fleet
