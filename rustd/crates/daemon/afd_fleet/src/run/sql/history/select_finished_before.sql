-- One page of a fleet thread's finished turns before a cursor, newest first: what a chat run's
-- history is read from, a page at a time until the thread or the run frame runs out. Finished
-- is the statement's predicate, so a running or refused row never takes a finished one's place
-- in a page. Walks `idx_turns_thread_id_created_at_id`.
--
-- $1 workspace · $2 fleet · $3 cursor instant · $4 cursor turn · $5 the processed status ·
-- $6 the failed status · $7 page size
SELECT id AS turn_id, created_at, sequence, status, failure_reason,
       request_json::text AS request_json, answer_text
  FROM thread.turns
 WHERE workspace_id = $1 AND thread_id = $2 AND opened_at IS NOT NULL
   AND (created_at, id) < ($3::timestamptz, $4)
   AND status IN ($5, $6)
 ORDER BY created_at DESC, id DESC
 LIMIT $7
