UPDATE runner.decision_payloads SET result = $2, updated_at = clock_timestamp()
WHERE decision_call_id = $1
