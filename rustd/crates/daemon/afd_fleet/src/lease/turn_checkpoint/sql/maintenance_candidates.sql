SELECT h.event_id, a.fleet_id, a.workspace_id
FROM runner.turn_recovery h JOIN fleet.admissions a ON a.id=h.event_id
WHERE (a.settled_at<=statement_timestamp()-make_interval(secs=>$1)
       AND NOT EXISTS (SELECT 1 FROM runner.turn_checkpoints p JOIN runner.turn_checkpoint_pins pin ON pin.turn_checkpoint_id=p.id
           WHERE p.event_id=h.event_id AND pin.released_at IS NULL
             AND pin.expires_at>statement_timestamp() AND pin.operation_expires_at>statement_timestamp()))
   OR EXISTS (SELECT 1 FROM runner.turn_checkpoints p WHERE p.event_id=h.event_id
       AND p.ready_at IS NULL AND (p.abandoned_at IS NOT NULL OR p.expires_at<=statement_timestamp())
       AND NOT EXISTS (SELECT 1 FROM runner.turn_checkpoint_pins pin WHERE pin.turn_checkpoint_id=p.id
           AND pin.released_at IS NULL AND pin.expires_at>statement_timestamp() AND pin.operation_expires_at>statement_timestamp()))
ORDER BY h.updated_at,h.event_id LIMIT $2
