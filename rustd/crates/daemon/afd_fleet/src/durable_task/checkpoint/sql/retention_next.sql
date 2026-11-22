UPDATE fleet.durable_tasks t SET checkpoint_cleanup_at = (
    SELECT min(due.at) FROM (
        SELECT c.expires_at AS at FROM runner.checkpoints c WHERE c.durable_task_id = t.id AND c.ready_at IS NULL AND c.abandoned_at IS NULL
        UNION ALL
        SELECT p.expires_at FROM runner.checkpoint_pins p JOIN runner.checkpoints c ON c.id = p.checkpoint_id
        WHERE c.durable_task_id = t.id AND p.released_at IS NULL AND p.expires_at > clock_timestamp()
        UNION ALL
        SELECT t.terminal_at + make_interval(secs => $2) WHERE t.terminal_at > clock_timestamp() - make_interval(secs => $2)
            AND EXISTS (SELECT 1 FROM runner.checkpoints c WHERE c.durable_task_id = t.id)
    ) due
), updated_at = now() WHERE t.id = $1
