WITH current_state AS (
  SELECT id, status FROM runner.runners WHERE id = $1::uuid FOR UPDATE
), updated AS (
  UPDATE runner.runners r
  SET selftest_requested_at = now(), updated_at = now()
  FROM current_state c
  WHERE r.id = c.id AND c.status <> $2::text
  RETURNING r.selftest_requested_at
)
SELECT c.status, (SELECT selftest_requested_at FROM updated) AS requested_at
FROM current_state c
