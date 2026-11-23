-- A requested drain finishes once the runner holds no live run. The caller's
-- compare-and-set already holds the row, so the clock is read after the lock.
-- $1 runner · $2 draining · $3 drained · $4 the active run status
UPDATE runner.runners r SET status = $3, updated_at = now()
WHERE r.id = $1 AND r.status = $2
  AND NOT EXISTS (
    SELECT 1 FROM thread.runs l
    WHERE l.runner_id = r.id AND l.status = $4 AND l.run_expires_at > clock_timestamp()
  )
RETURNING r.id
