-- The source turn keeps its accepted plan; `trg_turns_refuse_plan_change` refuses a second one.
UPDATE thread.turns SET plan = $4, plan_digest = $5, updated_at = clock_timestamp()
WHERE id = $1 AND thread_id = $2 AND workspace_id = $3
