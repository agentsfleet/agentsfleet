-- The source turn keeps its plan, written once with the plan's items; `trg_turns_refuse_plan_change`
-- refuses a second. An answer receipt for the turn reads `interpreted_at`.
UPDATE thread.turns SET plan = $4, plan_digest = $5,
    interpreted_at = COALESCE(interpreted_at, clock_timestamp()), updated_at = clock_timestamp()
WHERE id = $1 AND thread_id = $2 AND workspace_id = $3
