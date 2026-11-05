DELETE FROM fleet.task_completions WHERE id IN (
    SELECT id FROM fleet.lock_expired_task_completions($1::interval, $2::bigint)
)
