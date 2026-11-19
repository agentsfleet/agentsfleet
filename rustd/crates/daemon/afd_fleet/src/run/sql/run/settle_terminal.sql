-- Only a turn whose event opened and ended: `$4` is the status an open event holds.
UPDATE thread.turns a
SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now()
WHERE a.thread_id = $1 AND a.id = $2 AND a.run_fence = $3
  AND a.settled_at IS NULL
  AND a.opened_at IS NOT NULL AND a.status <> $4
