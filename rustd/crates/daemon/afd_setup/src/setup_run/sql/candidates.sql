-- Drafts whose head message a draft-owned run may take, oldest head first. A hint only:
-- the assignment rereads every fact under the draft's lock.
--
-- The head is the oldest message still queued or running; a stopped one is cancelled. A running
-- head is taken again once its thread holds no live run: its run lapsed, or its runner was
-- deleted and the run went with it (`thread.runs.runner_id` cascades), so this assignment
-- reassigns it. Without the second, a deleted runner's head would wait forever.
--
-- $1 the editing draft status · $2 the queued message status · $3 the running message status ·
-- $4 limit · $5 the polling runner
WITH host AS MATERIALIZED (
  -- A tenant's runner is assigned only its tenant's drafts, and an unknown runner none.
  SELECT platform_owned, tenant_id FROM runner.runners WHERE id = $5
), waiting AS (
  -- A setup turn has no producer; an unsettled one is queued or running.
  SELECT DISTINCT m.thread_id AS draft_id
    FROM thread.turns m
   WHERE m.status IN ($2, $3) AND m.producer IS NULL AND m.settled_at IS NULL
)
SELECT d.id AS id, d.workspace_id AS workspace_id, d.tenant_id AS tenant_id,
       d.user_id AS user_id
  FROM waiting w
  JOIN setup.drafts d ON d.id = w.draft_id
  JOIN host h ON h.platform_owned OR h.tenant_id = d.tenant_id
  CROSS JOIN LATERAL (
    SELECT m.tenant_model_id, m.created_at
      FROM thread.turns m
     WHERE m.thread_id = d.id AND m.status IN ($2, $3) AND m.settled_at IS NULL
     ORDER BY m.sequence
     LIMIT 1
  ) head
 WHERE d.status = $1
   -- A tenant runner never receives the platform model key. Tested on the head, so a head
   -- on the platform default blocks its draft for this runner instead of being jumped, as
   -- the fleet assignment's head does (`afd_fleet/.../assign_turn_head.sql`).
   AND (h.platform_owned OR head.tenant_model_id IS NOT NULL)
   -- One live run per draft: its thread's id is the draft's.
   AND NOT EXISTS (SELECT 1 FROM thread.threads s
                    WHERE s.id = d.id AND s.run_expires_at > statement_timestamp())
 ORDER BY head.created_at, d.id
 LIMIT $4
