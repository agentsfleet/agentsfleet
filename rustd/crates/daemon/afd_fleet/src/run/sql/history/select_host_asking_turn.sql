-- The action is the parsed key of the trusted QuestionAnswer producer. The
-- asking id must independently agree with the gate the action resolved.
SELECT asking.id, asking.sequence, asking.request_json::text,
       COALESCE(asking.answer_text, g.proposed_action)
  FROM fleet.approval_gates g
  JOIN thread.turns asking
    ON asking.id = g.turn_id AND asking.thread_id = g.fleet_id
   AND asking.workspace_id = g.workspace_id
 WHERE g.workspace_id = $1 AND g.fleet_id = $2 AND g.action_id = $3 AND g.turn_id = $4
   AND g.gate_type = $5 AND g.evidence->>'kind' = $6 AND g.resolved_at IS NOT NULL
