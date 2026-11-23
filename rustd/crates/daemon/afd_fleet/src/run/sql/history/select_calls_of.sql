-- Every tool call a fleet's runs made for one turn of a history page, in the order they were
-- made: by the fence of the run that made them, then by their number in it. One turn at a time,
-- so the history read stops at the turn that passes the run frame. An item names the turn newest
-- when its step began, so a call goes with the message it answered. Walks `idx_items_turn_id`.
--
-- $1 workspace · $2 fleet · $3 the turn
SELECT turn_id, fencing_token, sequence, tool, status, arguments::text AS arguments, output
  FROM thread.items
 WHERE workspace_id = $1 AND fleet_id = $2 AND turn_id = $3 AND tool IS NOT NULL
 ORDER BY fencing_token, sequence
