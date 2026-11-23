UPDATE thread.turns a SET head_checkpoint_id = NULL, next_checkpoint_generation = NULL
WHERE a.id=$1 AND a.next_checkpoint_generation IS NOT NULL
AND a.settled_at<=$2::timestamptz-make_interval(secs=>$3)
AND NOT EXISTS (SELECT 1 FROM runner.checkpoints p WHERE p.turn_id=a.id)
AND NOT EXISTS (SELECT 1 FROM thread.replay_receipts r WHERE r.scope=$4)
