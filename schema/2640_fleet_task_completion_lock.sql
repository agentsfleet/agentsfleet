-- Preserve parent-completion -> child-fanout lock order during bounded GC.
-- api_runtime can delete receipts but cannot UPDATE this immutable table, so
-- it cannot take the required explicit row locks without this narrow helper.
-- Canonical runtime limits: task::waits::cleanup::{COMPLETION_RETENTION, COMPLETION_BATCH}.
CREATE FUNCTION fleet.lock_expired_task_completions(retention INTERVAL, batch_limit BIGINT)
RETURNS TABLE (id UUID)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp AS $$
    SELECT c.id FROM fleet.task_completions c
    JOIN fleet.task_completion_fanout p ON p.completion_id = c.id
    WHERE retention > INTERVAL '0 seconds' AND batch_limit > 0
      AND c.received_at <= statement_timestamp() - GREATEST(retention, INTERVAL '72 hours')
      AND p.finished_at IS NOT NULL
    ORDER BY c.received_at, c.id
    LIMIT LEAST(GREATEST(batch_limit, 0), 256)
    FOR UPDATE OF c SKIP LOCKED
$$;
REVOKE ALL ON FUNCTION fleet.lock_expired_task_completions(INTERVAL, BIGINT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fleet.lock_expired_task_completions(INTERVAL, BIGINT) TO api_runtime;
