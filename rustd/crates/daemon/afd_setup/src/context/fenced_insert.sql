-- A setup run's first save: the insert `insert.sql` makes, written only while the run holds the
-- draft's thread at its fence, share-locked for the reason `fenced_update.sql` gives.
--
-- $1 id · $2 draft · $3 version · $4 document · $5 expected version · $6 run · $7 runner ·
-- $8 fence · $9 the live run status
-- Lock order is the thread, then the run, as renewal (`renew_batch.sql`) and settlement
-- (`settle_run.sql`) take them: each is its own `MATERIALIZED` step, so the run is locked
-- only once the thread is, and a save never waits on a renewal that waits on it.
WITH locked_thread AS MATERIALIZED (
    SELECT t.id, t.fencing_token
      FROM thread.threads t
     WHERE t.id = $2 AND t.fencing_token = $8
       FOR SHARE OF t
), holder AS MATERIALIZED (
    SELECT r.id
      FROM thread.runs r
      JOIN locked_thread t ON t.id = r.draft_id AND t.fencing_token = r.fencing_token
     WHERE r.id = $6 AND r.draft_id = $2 AND r.runner_id = $7 AND r.fencing_token = $8
       AND r.status = $9
       FOR SHARE OF r
)
INSERT INTO memory.context_checkpoints
    (id, draft_id, resource_version, document, created_at, updated_at)
SELECT $1, $2, $3, $4, now(), now()
  FROM holder
 WHERE $5::bigint = 0
ON CONFLICT DO NOTHING
