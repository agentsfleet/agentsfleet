WITH host AS MATERIALIZED (
  -- The polling runner's owner, read once: a tenant's runner is offered only its
  -- tenant's drafts, and an unknown runner none. `admit` re-checks under the lock.
  SELECT platform_owned, tenant_id FROM runner.runners WHERE id = $5
)
SELECT d.id AS id,
       d.workspace_id AS workspace_id,
       d.tenant_id AS tenant_id,
       d.user_id AS user_id
  FROM authoring.drafts d
  JOIN authoring.messages m ON m.draft_id = d.id
  JOIN host h ON h.platform_owned OR h.tenant_id = d.tenant_id
 WHERE d.status = $1 AND m.status = $2 AND m.sequence > d.cancelled_through_sequence
   -- Only the newest attempt above the watermark can still be running, and
   -- reading it newest-first keeps the draft's sequence index in charge.
   AND NOT EXISTS (SELECT 1
                     FROM (SELECT a.status FROM authoring.attempts a
                            WHERE a.draft_id = d.id
                              AND a.message_sequence > d.cancelled_through_sequence
                            ORDER BY a.message_sequence DESC
                            LIMIT 1) latest
                    WHERE latest.status <> ALL($3::text[]))
 GROUP BY d.id
 ORDER BY min(m.created_at), d.id
 LIMIT $4
