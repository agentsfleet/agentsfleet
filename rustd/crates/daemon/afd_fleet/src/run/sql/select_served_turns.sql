-- Which of the turns a post's records name the run serves: its own, opened, or one its
-- conversation folded in at its fence. Each named turn is read by its key.
SELECT a.id
FROM thread.turns a
WHERE a.id = ANY($4::uuid[]) AND a.thread_id = $1
  AND ((a.id = $5 AND a.opened_at IS NOT NULL)
       OR (a.input_run_id = $2 AND a.input_fence = $3))
