SELECT accounting_digest, acknowledgement::text AS acknowledgement
  FROM billing.authoring_report_receipts
 WHERE attempt_id = $1::uuid AND report_id = $2::uuid
