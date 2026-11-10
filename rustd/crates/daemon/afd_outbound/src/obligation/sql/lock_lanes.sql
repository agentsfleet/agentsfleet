-- The lanes a claim may take heads from, each locked until the claim's
-- transaction ends. The heads are chosen by the next statement, whose snapshot
-- is taken after these locks, and a claim commits before it lets them go, so
-- every claim of a lane sees the one before it: a row that commits late cannot
-- be taken beside a head another claim is still writing. A lane another claim
-- holds is skipped; a hash collision between two lanes only skips one a pass.
WITH candidates AS MATERIALIZED (
    SELECT o.provider, o.workspace_id
      FROM fleet.obligations o
     WHERE o.delivered_at IS NULL AND o.abandoned_at IS NULL
       AND o.claim_expires_at <= statement_timestamp()
       AND (o.next_attempt_at IS NULL OR o.next_attempt_at <= statement_timestamp())
       AND NOT EXISTS (
           SELECT 1
             FROM fleet.obligations e
            WHERE e.provider = o.provider
              AND e.workspace_id = o.workspace_id
              AND e.delivered_at IS NULL AND e.abandoned_at IS NULL
              AND (e.sequence < o.sequence
                   OR (e.id <> o.id AND e.claim_expires_at > statement_timestamp())))
     ORDER BY o.sequence
)
-- Filtered above the materialised sort, row by row until the limit, so a
-- claim locks only the lanes it returns.
SELECT c.provider, c.workspace_id
  FROM candidates c
 WHERE pg_try_advisory_xact_lock(hashtext(c.provider), uuid_hash(c.workspace_id))
 LIMIT $1
