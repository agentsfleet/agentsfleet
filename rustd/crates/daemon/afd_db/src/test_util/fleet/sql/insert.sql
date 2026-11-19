WITH fleet AS (
    INSERT INTO fleet.fleets (id, workspace_id, tenant_id, name, status, required_tags, memory_reads_workspace, memory_publishes_workspace, resource_version, created_at, updated_at)
    VALUES ($1, $2, $3, $4, $5, $6::text[], $7, $8, $9, coalesce($10::timestamptz, now()), coalesce($11::timestamptz, $10::timestamptz, now()))
    RETURNING id, created_at
)
-- Every turn names its thread, so a fleet's thread is written with it, as creation writes it.
INSERT INTO thread.threads (id, fleet_id, fencing_token, run_expires_at, created_at, updated_at)
SELECT id, id, 0, clock_timestamp() - interval '1 millisecond', created_at, created_at FROM fleet
