SELECT request_json->>'message' AS user_text FROM thread.turns WHERE thread_id = $1 AND sequence = $2
