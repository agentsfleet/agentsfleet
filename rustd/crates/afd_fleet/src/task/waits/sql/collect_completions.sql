DELETE FROM fleet.task_completions WHERE id IN (
    SELECT c.id FROM fleet.task_completions c JOIN fleet.task_completion_fanout p ON p.completion_id = c.id
    WHERE c.received_at <= statement_timestamp() - $1::interval AND p.finished_at IS NOT NULL
    ORDER BY c.received_at, c.id LIMIT $2 FOR UPDATE OF c SKIP LOCKED
)
