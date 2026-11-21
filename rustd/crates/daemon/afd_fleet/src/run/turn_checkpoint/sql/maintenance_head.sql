DELETE FROM runner.turn_recovery h WHERE h.turn_id=$1
AND EXISTS (SELECT 1 FROM thread.turns a WHERE a.id=$1 AND a.settled_at<=$2::timestamptz-make_interval(secs=>$3))
AND NOT EXISTS (SELECT 1 FROM runner.turn_checkpoints p WHERE p.turn_id=h.turn_id)
RETURNING turn_id
