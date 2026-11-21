-- A run's save: the `resource_version` compare-and-swap `update.sql` makes, written only while
-- the run holds its thread at its fence. The thread and run rows are share-locked, so a
-- reassignment raising the fence waits for this save, or this save, having waited on it,
-- rereads the raised fence and writes nothing. Zero rows is a stale fence or a stale version;
-- the caller tells them apart. A run names its thread by its owner,
-- `COALESCE(fleet_id, draft_id)`, as the statements both kinds share do.
--
-- $1 thread · $2 new version · $3 document · $4 expected version · $5 run · $6 runner ·
-- $7 fence · $8 the live run status
-- Lock order is the thread, then the run, as renewal (`renew_batch.sql`) and settlement
-- (`settle_run.sql`) take them: each is its own `MATERIALIZED` step, so the run is locked
-- only once the thread is, and a save never waits on a renewal that waits on it.
WITH locked_thread AS MATERIALIZED (
    SELECT t.id, t.fencing_token
      FROM thread.threads t
     WHERE t.id = $1 AND t.fencing_token = $7
       FOR SHARE OF t
), holder AS MATERIALIZED (
    SELECT r.id
      FROM thread.runs r
      JOIN locked_thread t
        ON t.id = COALESCE(r.fleet_id, r.draft_id) AND t.fencing_token = r.fencing_token
     WHERE r.id = $5 AND COALESCE(r.fleet_id, r.draft_id) = $1 AND r.runner_id = $6
       AND r.fencing_token = $7 AND r.status = $8
       FOR SHARE OF r
)
UPDATE memory.context_checkpoints c
   SET resource_version = $2, document = $3, updated_at = now()
  FROM holder
 WHERE c.thread_id = $1 AND c.resource_version = $4
