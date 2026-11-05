WITH inserted AS (
    INSERT INTO fleet.task_completions
        (id, workspace_id, scope_fleet_id, installation_id, repository_id, head_oid,
         workflow_id, run_id, run_number, run_attempt, conclusion, body_digest, received_at, created_at)
    SELECT $1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $15, $9, $10, $11, clock_timestamp(), now()
    WHERE EXISTS (SELECT 1 FROM fleet.tasks t
        WHERE t.workspace_id = $2::uuid AND ($3::uuid IS NULL OR t.fleet_id = $3::uuid)
          AND t.target @> jsonb_build_object($12::text, $4::bigint, $13::text, $5::bigint, $14::text, $6::text)
          AND t.terminal_at IS NULL)
    ON CONFLICT (workspace_id, scope_fleet_id, installation_id, repository_id, run_id, run_attempt) DO NOTHING
    RETURNING id
), progress AS (
    INSERT INTO fleet.task_completion_fanout (completion_id, next_attempt_at, created_at, updated_at)
    SELECT id, clock_timestamp(), now(), now() FROM inserted RETURNING completion_id
)
SELECT count(*)::bigint FROM progress
