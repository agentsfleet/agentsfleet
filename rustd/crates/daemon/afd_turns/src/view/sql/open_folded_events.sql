-- Opens, as received, the event of each admitted turn a run took in, joined to the run
-- whose answer it takes. A turn keeps its own instant, so a thread page that listed it
-- waiting lists it at the same place. A turn already opened keeps its history and learns
-- the run it joined if it named none.
--
-- $1 fleet · $2 the turns' ids · $3 the run's turn · $4 received status
UPDATE thread.turns t
SET status = CASE WHEN t.opened_at IS NULL THEN $4 ELSE t.status END,
    resumes_turn_id = COALESCE(t.resumes_turn_id, $3),
    updated_at = now(),
    opened_at = COALESCE(t.opened_at, now())
WHERE t.thread_id = $1 AND t.id = ANY($2)
