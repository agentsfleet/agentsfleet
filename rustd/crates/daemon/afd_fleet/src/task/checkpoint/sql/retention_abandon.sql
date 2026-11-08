UPDATE fleet.task_checkpoints c SET abandoned_at = clock_timestamp(), updated_at = now()
FROM fleet.tasks t
WHERE c.task_id = $1 AND t.id = c.task_id AND c.ready_at IS NULL AND c.abandoned_at IS NULL
  AND (c.transfer_expires_at <= clock_timestamp() OR t.generation <> c.task_generation
       OR t.stop_generation <> c.stop_generation OR t.terminal_at IS NOT NULL
       OR NOT EXISTS (SELECT 1 FROM runner.leases l JOIN runner.fleet_slots s ON s.fleet_id = l.fleet_id
           WHERE l.id = c.owner_lease_id AND l.fencing_token = c.owner_fence AND l.status = $2
             AND s.fencing_token = l.fencing_token AND s.last_runner_id = l.runner_id
             AND l.lease_expires_at > clock_timestamp() AND s.lease_expires_at > clock_timestamp()))
