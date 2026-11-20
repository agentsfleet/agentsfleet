-- A setup run's first save: the insert `insert.sql` makes, written only while the run holds the
-- draft's thread at its fence, share-locked for the reason `fenced_update.sql` gives.
--
-- $1 id · $2 draft · $3 version · $4 document · $5 expected version · $6 run · $7 runner ·
-- $8 fence · $9 the live run status
WITH holder AS (
    SELECT r.id
      FROM thread.runs r
      JOIN thread.threads t ON t.id = r.draft_id AND t.fencing_token = r.fencing_token
     WHERE r.id = $6 AND r.draft_id = $2 AND r.runner_id = $7 AND r.fencing_token = $8
       AND r.status = $9
       FOR SHARE OF r, t
)
INSERT INTO memory.context_checkpoints
    (id, draft_id, resource_version, document, created_at, updated_at)
SELECT $1, $2, $3, $4, now(), now()
  FROM holder
 WHERE $5::bigint = 0
ON CONFLICT DO NOTHING
