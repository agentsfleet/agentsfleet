SELECT user_text, answer_text
  FROM authoring.messages
 WHERE draft_id = $1::uuid AND sequence <= $2
 ORDER BY sequence
 LIMIT $3
