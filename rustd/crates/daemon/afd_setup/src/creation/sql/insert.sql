-- A new fleet and its thread commit together: every turn names its thread, so the thread exists
-- before the first message. A fresh thread holds no run, so its deadline has passed. A name the
-- workspace already holds inserts neither, and the caller answers that.
--
-- $1 id · $2 workspace · $3 tenant · $4 name · $5 status · $6 model · $7 HTTP egress ·
-- $8 package domains · $9 resource version
WITH fleet AS (
    INSERT INTO fleet.fleets
        (id, workspace_id, tenant_id, name, status, model_ref, http_egress, package_domains,
         required_tags, memory_reads_workspace, memory_publishes_workspace, resource_version,
         created_at, updated_at)
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
            ARRAY[]::text[], false, false, $9, now(), now())
    ON CONFLICT (workspace_id, name) DO NOTHING
    RETURNING id, created_at
), thread AS (
    INSERT INTO thread.threads
        (id, fleet_id, fencing_token, run_expires_at, created_at, updated_at)
    SELECT id, id, 0, clock_timestamp() - interval '1 millisecond', created_at, created_at
      FROM fleet
    RETURNING id
)
SELECT id FROM thread
