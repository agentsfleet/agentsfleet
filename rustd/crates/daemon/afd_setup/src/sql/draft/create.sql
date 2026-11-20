-- The draft and its thread commit together: every turn names its thread, so the thread
-- exists before the first message. A fresh thread holds no run, so its deadline has passed.
WITH draft AS (
    INSERT INTO setup.drafts
        (id, workspace_id, tenant_id, user_id, title, status, tenant_model_id,
         source_type, source_plugin_revision_id, source_fleet_revision_id, instance_name,
         fleet_id, last_message_sequence, resource_version,
         created_at, updated_at)
    VALUES ($1, $2, $3, $4, $5, $6, $7,
            $8, $9, $10, $11,
            $12, $13, $13,
            now(), now())
    RETURNING id, created_at
)
INSERT INTO thread.threads
    (id, draft_id, fencing_token, run_expires_at, created_at, updated_at)
SELECT id, id, 0, clock_timestamp() - interval '1 millisecond', created_at, created_at
FROM draft
