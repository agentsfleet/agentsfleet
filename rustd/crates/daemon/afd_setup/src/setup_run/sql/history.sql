-- The draft's earlier answered turns, newest first: every one an assignment carries, each with
-- its sequence, which a saved context's watermark counts in, its message and answer. Only the
-- run frame bounds what the assignment sends (`afd_fleet::run::history::fit_frame`).
--
-- $1 draft · $2 the message the run runs · $3 the completed status
SELECT sequence, request_json->>'message' AS user_text, answer_text
  FROM thread.turns
 WHERE thread_id = $1 AND sequence < $2 AND status = $3 AND answer_text IS NOT NULL
 ORDER BY sequence DESC
