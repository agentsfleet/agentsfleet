-- Whether a run still holds the draft's thread at its fence, read after a fenced save wrote
-- nothing, to tell a stale fence from a stale version.
--
-- $1 draft · $2 run · $3 runner · $4 fence · $5 the live run status
SELECT EXISTS (
    SELECT 1
      FROM thread.runs r
      JOIN thread.threads t ON t.id = r.draft_id AND t.fencing_token = r.fencing_token
     WHERE r.id = $2 AND r.draft_id = $1 AND r.runner_id = $3 AND r.fencing_token = $4
       AND r.status = $5
)
