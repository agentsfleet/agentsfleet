SELECT accounting_digest, acknowledgement::text AS acknowledgement
  FROM billing.setup_report_receipts
 WHERE attempt_id = $1 AND report_id = $2
