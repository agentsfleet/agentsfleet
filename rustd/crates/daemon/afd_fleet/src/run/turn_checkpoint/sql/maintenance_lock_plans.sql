SELECT p.id FROM runner.turn_checkpoints p
WHERE p.event_id=$1 AND (
    (p.ready_at IS NULL AND (p.abandoned_at IS NOT NULL OR p.expires_at<=statement_timestamp()))
    OR EXISTS (SELECT 1 FROM fleet.admissions a WHERE a.id=$1
        AND a.settled_at<=statement_timestamp()-make_interval(secs=>$3)))
AND NOT EXISTS (SELECT 1 FROM runner.turn_checkpoint_pins pin WHERE pin.turn_checkpoint_id=p.id
    AND pin.released_at IS NULL AND pin.expires_at>statement_timestamp()
    AND pin.operation_expires_at>statement_timestamp())
ORDER BY p.generation LIMIT $2 FOR UPDATE OF p
