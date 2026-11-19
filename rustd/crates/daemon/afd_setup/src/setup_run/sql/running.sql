-- The message a draft run takes, running from its claim on. A reclaimed message is
-- already running and matches nothing.
--
-- $1 draft · $2 sequence · $3 the running status · $4 the queued status
UPDATE thread.turns
   SET status = $3, updated_at = now()
 WHERE thread_id = $1 AND sequence = $2 AND status = $4
