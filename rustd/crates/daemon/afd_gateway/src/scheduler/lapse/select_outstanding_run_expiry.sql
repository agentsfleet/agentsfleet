-- Boot only: the instant the last outstanding run lapses. A run already past its
-- deadline is covered by the pass the wake subscription requests when it opens.
SELECT max(run_expires_at) FROM thread.runs
WHERE status = $1 AND run_expires_at > statement_timestamp()
