-- The ledger records the producer and key at admission, before waking runners.
-- The continuation view's resumes_turn_id may still be getting materialized.
SELECT producer_key FROM thread.turns
 WHERE workspace_id = $1 AND thread_id = $2 AND id = $3 AND producer = $4
