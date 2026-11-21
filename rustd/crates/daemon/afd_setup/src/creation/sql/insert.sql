-- The fleet and its thread commit together: every turn names its thread, so the thread
-- exists before the first input. A fresh thread holds no run, so its deadline has passed.
WITH fleet AS (
    INSERT INTO fleet.fleets
        (id, workspace_id, tenant_id, name, status, head_revision_id, required_tags,
         setup_stream_ready, initial_start, memory_reads_workspace, memory_publishes_workspace,
         resource_version, created_at, updated_at)
    VALUES ($1, $2, $3, $4, $5, $6, $7::text[],
            false, $8::jsonb, false, false,
            $9, now(), now())
    RETURNING id, created_at
)
INSERT INTO thread.threads
    (id, fleet_id, fencing_token, run_expires_at, created_at, updated_at)
SELECT id, id, 0, clock_timestamp() - interval '1 millisecond', created_at, created_at
FROM fleet
