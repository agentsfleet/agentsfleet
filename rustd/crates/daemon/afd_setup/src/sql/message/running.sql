SELECT sequence
  FROM authoring.messages
 WHERE draft_id = $1 AND status = $2
 ORDER BY sequence
 LIMIT 1
