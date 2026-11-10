SELECT request, result FROM runner.decision_payloads
WHERE decision_call_id = $1
