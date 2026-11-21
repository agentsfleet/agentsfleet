-- A draft-owned run's facts its renewal and report price and attribute by: who pays, at
-- which posture and model, the draft and the message it runs, its fence and its state. No
-- row for a fleet's run or another runner's.
--
-- $1 run · $2 runner
SELECT l.draft_id, l.turn_id, l.workspace_id, l.tenant_id, l.posture, l.provider, l.model,
       l.fencing_token, l.status, l.created_at,
       l.cancel_requested_at IS NOT NULL AS cancelled
FROM thread.runs l
WHERE l.id = $1 AND l.runner_id = $2 AND l.draft_id IS NOT NULL
