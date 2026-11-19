-- Drafts whose head message a draft-owned run may take, oldest head first. A hint only:
-- the claim rereads every fact under the draft's lock.
--
-- The head is the oldest message above the Stop watermark still queued or running. A running
-- head is taken again only when a draft run ran it before (`thread.runs.event_id` is the
-- message): that run lapsed, so this claim reoffers it. A running head the authoring
-- attempt path owns waits for that path.
--
-- $1 the editing draft status · $2 the queued message status · $3 the running message status ·
-- $4 the attempt statuses that no longer hold a draft · $5 limit · $6 the polling runner
WITH host AS MATERIALIZED (
  -- A tenant's runner is offered only its tenant's drafts, and an unknown runner none.
  SELECT platform_owned, tenant_id FROM runner.runners WHERE id = $6
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
    SELECT m.id, m.status, m.tenant_model_id, m.created_at
      FROM thread.turns m
     WHERE m.thread_id = d.id AND m.sequence > d.cancelled_through_sequence
       AND m.status IN ($2, $3)
     ORDER BY m.sequence
     LIMIT 1
  ) head
 WHERE d.status = $1
   AND (head.status = $2
        OR EXISTS (SELECT 1 FROM thread.runs l
                    WHERE l.event_id = head.id AND l.draft_id = d.id))
   -- A tenant runner never receives the platform model key. Tested on the head, so a head
   -- on the platform default blocks its draft for this runner instead of being jumped, as
   -- the fleet claim's head does (`afd_fleet/.../claim_admission_head.sql`).
   AND (h.platform_owned OR head.tenant_model_id IS NOT NULL)
   -- One live run per draft: its slot's id is the draft's.
   AND NOT EXISTS (SELECT 1 FROM thread.threads s
                    WHERE s.id = d.id AND s.run_expires_at > statement_timestamp())
   -- Setup turns the authoring path still runs; this term goes with setup.attempts.
   AND NOT EXISTS (SELECT 1 FROM setup.attempts a
                    WHERE a.draft_id = d.id
                      AND a.message_sequence > d.cancelled_through_sequence
                      AND a.status <> ALL($4::text[]))
 ORDER BY head.created_at, d.id
 LIMIT $5
