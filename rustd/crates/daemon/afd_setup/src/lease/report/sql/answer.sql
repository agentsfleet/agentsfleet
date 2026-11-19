SELECT answer_digest
  FROM authoring.answer_receipts
 WHERE attempt_id = $1 AND report_id = $2
   AND expires_at > statement_timestamp()
