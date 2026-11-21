-- Every tool call a fleet's runs made for the turns a history page holds, each turn's in the
-- order they were made: by the fence of the run that made them, then by their number in it.
-- An item names the turn newest when its step began, so a call goes with the message it
-- answered. Walks `idx_items_turn_id`.
--
-- $1 workspace · $2 fleet · $3 the page's turns
SELECT turn_id, fencing_token, sequence, tool, status, arguments::text AS arguments, output
  FROM thread.items
 WHERE workspace_id = $1 AND fleet_id = $2 AND turn_id = ANY($3::uuid[]) AND tool IS NOT NULL
 ORDER BY turn_id, fencing_token, sequence
