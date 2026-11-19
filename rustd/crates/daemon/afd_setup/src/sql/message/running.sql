SELECT sequence
  FROM setup.messages
 WHERE draft_id = $1 AND status = $2
 ORDER BY sequence
 LIMIT 1
