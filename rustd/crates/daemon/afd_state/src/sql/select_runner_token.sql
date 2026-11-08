SELECT id AS runner, status, degraded_reason IS NOT NULL AS degraded
FROM runner.runners
WHERE token_hash = $1
LIMIT 1
