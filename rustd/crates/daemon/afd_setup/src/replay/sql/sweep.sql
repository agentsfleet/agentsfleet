DELETE FROM setup.replay_receipts
 WHERE id IN (
    SELECT id FROM setup.replay_receipts
     WHERE created_at <= statement_timestamp() - $1::interval
     ORDER BY created_at, id
     LIMIT $2
)
