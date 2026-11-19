SELECT sequence, request_json->>'message' AS user_text, answer_text FROM thread.turns
 WHERE thread_id = $1 AND sequence > $2 AND sequence < $3
 ORDER BY sequence LIMIT $4
