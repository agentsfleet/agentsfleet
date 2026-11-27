INSERT INTO thread.run_web_calls (run_id, vendor, operation, call_count, created_at, updated_at)
VALUES ($1, $2, $3, 1, now(), now())
ON CONFLICT (run_id, vendor, operation)
DO UPDATE SET call_count = thread.run_web_calls.call_count + 1, updated_at = now()
