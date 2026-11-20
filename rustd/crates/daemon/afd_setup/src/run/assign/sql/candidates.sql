WITH host AS MATERIALIZED (
  -- The polling runner's owner, read once: a tenant's runner is assigned only its
  -- tenant's drafts, and an unknown runner none. `assign_candidate` re-checks under the lock.
  SELECT platform_owned, tenant_id FROM runner.runners WHERE id = $5
)
SELECT d.id AS id,
       d.workspace_id AS workspace_id,
       d.tenant_id AS tenant_id,
       d.user_id AS user_id
  FROM setup.drafts d
  JOIN thread.turns m ON m.thread_id = d.id
  JOIN host h ON h.platform_owned OR h.tenant_id = d.tenant_id
 -- A stopped turn is settled, so the draft's unsettled index skips its history.
 WHERE d.status = $1 AND m.status = $2 AND m.settled_at IS NULL
   -- Only the newest attempt can still be running, and reading it newest-first
   -- keeps the draft's sequence index in charge. One whose turn Stop cancelled
   -- keeps only its accounting, not the draft.
   AND NOT EXISTS (SELECT 1
                     FROM (SELECT a.status, a.message_sequence FROM setup.attempts a
                            WHERE a.draft_id = d.id
                            ORDER BY a.message_sequence DESC
                            LIMIT 1) latest
                    WHERE latest.status <> ALL($3::text[])
                      AND NOT EXISTS (SELECT 1 FROM thread.turns c
                                       WHERE c.thread_id = d.id
                                         AND c.sequence = latest.message_sequence
                                         AND c.status = $6))
   -- Nor one a draft run holds: a live thread's id is its draft's.
   AND NOT EXISTS (SELECT 1 FROM thread.threads s
                    WHERE s.id = d.id AND s.run_expires_at > statement_timestamp())
 GROUP BY d.id
 ORDER BY min(m.created_at), d.id
 LIMIT $4
