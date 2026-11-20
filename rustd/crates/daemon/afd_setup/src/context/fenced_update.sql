-- A setup run's save: the `resource_version` compare-and-swap `update.sql` makes, written only
-- while the run holds the draft's thread at its fence. The run and thread rows are share-locked,
-- so a reassignment raising the fence waits for this save, or this save, having waited on it,
-- rereads the raised fence and writes nothing. Zero rows is a stale fence or a stale version;
-- the caller tells them apart.
--
-- $1 draft · $2 new version · $3 document · $4 expected version · $5 run · $6 runner · $7 fence ·
-- $8 the live run status
WITH holder AS (
    SELECT r.id
      FROM thread.runs r
      JOIN thread.threads t ON t.id = r.draft_id AND t.fencing_token = r.fencing_token
     WHERE r.id = $5 AND r.draft_id = $1 AND r.runner_id = $6 AND r.fencing_token = $7
       AND r.status = $8
       FOR SHARE OF r, t
)
UPDATE memory.context_checkpoints c
   SET resource_version = $2, document = $3, updated_at = now()
  FROM holder
 WHERE c.draft_id = $1 AND c.resource_version = $4
