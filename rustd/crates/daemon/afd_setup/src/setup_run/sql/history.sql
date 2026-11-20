-- The draft's earlier answered turns, newest first, for the assignment's bounded history:
-- each turn's sequence, which a saved context's watermark counts in, its message and answer.
--
-- $1 draft · $2 the message the run runs · $3 the completed status · $4 limit
SELECT sequence, request_json->>'message' AS user_text, answer_text
  FROM thread.turns
 WHERE thread_id = $1 AND sequence < $2 AND status = $3 AND answer_text IS NOT NULL
 ORDER BY sequence DESC
 LIMIT $4
