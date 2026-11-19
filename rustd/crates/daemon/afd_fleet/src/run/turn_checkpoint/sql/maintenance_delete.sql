DELETE FROM runner.turn_checkpoints p
WHERE p.event_id=$1 AND p.id=ANY($4::uuid[]) AND (
    (p.ready_at IS NULL AND (p.abandoned_at IS NOT NULL OR p.expires_at<=$2::timestamptz))
    OR EXISTS (SELECT 1 FROM fleet.admissions a WHERE a.id=$1 AND a.settled_at<=$2::timestamptz-make_interval(secs=>$3)))
AND NOT EXISTS (SELECT 1 FROM runner.turn_checkpoint_pins pin WHERE pin.turn_checkpoint_id=p.id
    AND pin.released_at IS NULL AND pin.expires_at>$2 AND pin.operation_expires_at>$2)
