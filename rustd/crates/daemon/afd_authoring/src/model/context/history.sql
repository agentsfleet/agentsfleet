SELECT sequence, user_text, answer_text FROM authoring.messages
 WHERE draft_id = $1::uuid AND sequence > $2 AND sequence < $3
 ORDER BY sequence LIMIT $4
