-- The facts an assignment reads about one candidate draft, in one statement so they come from
-- one snapshot, locking nothing: the draft's tenant and head candidate, and its head message,
-- the oldest still queued or running. A running head is reassigned whether or not a run row
-- still names it, since a runner's deletion leaves its lapsed run naming no runner
-- (`candidates.sql`).
--
-- These facts are hints. The thread claim (`afd_fleet`'s `assign_thread.sql`) and the turn
-- claim (`running.sql`) decide, and a fact that went stale since this read fails one of them.
--
-- No row when nothing here may be assigned now: the creator's draft is gone or left editing,
-- its thread holds a live run, or no message waits.
--
-- $1 draft · $2 workspace · $3 creator · $4 the editing status · $5 the queued status ·
-- $6 the running status
SELECT d.tenant_id, d.head_revision_id,
       m.id, m.sequence, m.status, m.tenant_model_id, m.request_json->>'message' AS user_text,
       m.save_requested, m.created_at
  FROM setup.drafts d
 CROSS JOIN LATERAL (
    SELECT t.id, t.sequence, t.status, t.tenant_model_id, t.request_json, t.save_requested,
           t.created_at
      FROM thread.turns t
     WHERE t.thread_id = d.id AND t.status IN ($5, $6) AND t.settled_at IS NULL
     ORDER BY t.sequence
     LIMIT 1
 ) m
 WHERE d.id = $1 AND d.workspace_id = $2 AND d.user_id = $3 AND d.status = $4
   -- One live run per draft: its thread's id is the draft's.
   AND NOT EXISTS (SELECT 1 FROM thread.threads s
                    WHERE s.id = d.id AND s.run_expires_at > statement_timestamp())
