DELETE FROM runner.turn_recovery h WHERE h.event_id=$1
AND EXISTS (SELECT 1 FROM fleet.admissions a WHERE a.id=$1 AND a.settled_at<=$2::timestamptz-make_interval(secs=>$3))
AND NOT EXISTS (SELECT 1 FROM runner.turn_checkpoints p WHERE p.event_id=h.event_id)
RETURNING event_id
