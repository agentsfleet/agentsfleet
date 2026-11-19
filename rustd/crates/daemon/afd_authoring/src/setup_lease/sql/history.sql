-- The draft's earlier answered turns, newest first, for the offer's bounded history.
--
-- $1 draft · $2 the message the lease runs · $3 the completed status · $4 limit
SELECT user_text, answer_text
  FROM authoring.messages
 WHERE draft_id = $1 AND sequence < $2 AND status = $3 AND answer_text IS NOT NULL
 ORDER BY sequence DESC
 LIMIT $4
