SELECT answer_digest
  FROM authoring.answer_receipts
 WHERE attempt_id = $1::uuid AND report_id = $2::uuid
   AND expires_at > statement_timestamp()
