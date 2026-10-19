WITH expired AS (
    SELECT id FROM authoring.replay_receipts
     WHERE created_at <= statement_timestamp() - $1::interval
     ORDER BY created_at, id
     LIMIT $2
       FOR UPDATE SKIP LOCKED
)
DELETE FROM authoring.replay_receipts r
 USING expired
 WHERE r.id = expired.id
