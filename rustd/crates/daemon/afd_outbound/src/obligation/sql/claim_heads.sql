WITH heads AS (
    SELECT o.id
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
              -- An earlier open row heads the lane; a live claim holds it.
              -- Sequence is assigned at insert, not commit, so a row that
              -- commits after a later one was claimed must still wait.
              AND (e.sequence < o.sequence
                   OR (e.id <> o.id AND e.claim_expires_at > statement_timestamp())))
     ORDER BY o.sequence
     LIMIT $1
     FOR UPDATE OF o SKIP LOCKED
)
UPDATE fleet.obligations o
   SET receipt          = $2,
       claim_expires_at = clock_timestamp() + $3::interval,
       updated_at       = now()
  FROM heads
 WHERE o.id = heads.id
RETURNING o.id, o.fleet_id, o.workspace_id, o.provider, o.destination, o.event_id, o.answer
