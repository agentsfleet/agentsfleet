WITH inserted AS (
    INSERT INTO fleet.durable_task_completions
        (id, workspace_id, scope_fleet_id, github_installation_id, github_repository_id, head_oid,
         github_workflow_id, github_run_id, run_number, run_attempt, received_at, created_at)
    SELECT $1, $2, $3, $4, $5, $6, $7, $8, $13, $9, clock_timestamp(), now()
    WHERE EXISTS (SELECT 1 FROM fleet.durable_tasks t
        WHERE t.workspace_id = $2 AND (t.fleet_id = $3 OR $3 IS NULL)
          AND t.target @> jsonb_build_object($10::text, $4::bigint, $11::text, $5::bigint, $12::text, $6::text)
          AND t.terminal_at IS NULL)
    ON CONFLICT (workspace_id, scope_fleet_id, github_installation_id, github_repository_id, github_run_id, run_attempt) DO NOTHING
    RETURNING id
), progress AS (
    INSERT INTO fleet.durable_task_completion_fanout (durable_task_completion_id, next_attempt_at, created_at, updated_at)
    SELECT id, clock_timestamp(), now(), now() FROM inserted RETURNING durable_task_completion_id
)
SELECT count(*)::bigint FROM progress
