DELETE FROM authoring.replay_receipts
 WHERE scope = $1 AND operation_id = $2::uuid
   AND created_at <= statement_timestamp() - $3::interval
