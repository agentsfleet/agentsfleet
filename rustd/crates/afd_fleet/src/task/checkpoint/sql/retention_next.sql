UPDATE fleet.tasks t SET checkpoint_cleanup_at = (
    SELECT min(due.at) FROM (
        SELECT c.transfer_deadline AS at FROM fleet.task_checkpoints c WHERE c.task_id = t.id AND c.ready_at IS NULL AND c.abandoned_at IS NULL
        UNION ALL
        SELECT p.expires_at FROM fleet.task_object_pins p WHERE p.task_id = t.id AND p.released_at IS NULL AND p.expires_at > clock_timestamp()
        UNION ALL
        SELECT t.terminal_at + make_interval(secs => $2) WHERE t.terminal_at > clock_timestamp() - make_interval(secs => $2)
            AND EXISTS (SELECT 1 FROM fleet.task_checkpoints c WHERE c.task_id = t.id)
    ) due
), updated_at = now() WHERE t.id = $1::uuid
