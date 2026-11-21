-- A draft run's turn, finished by its report: the first terminal write, so it matches only a
-- running turn and the one report that finishes it is the one that matches. A proposal the
-- report check refused keeps its sentence as the turn's failure detail beside the answer.
--
-- $1 draft · $2 sequence · $3 status · $4 answer · $5 candidate · $6 failure reason ·
-- $7 failure detail · $8 the running status
UPDATE thread.turns
   SET status = $3, answer_text = $4, candidate_id = $5, failure_reason = $6,
       failure_detail = $7, settled_at = now(), updated_at = now()
 WHERE thread_id = $1 AND sequence = $2 AND status = $8
