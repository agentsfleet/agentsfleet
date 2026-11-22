UPDATE thread.turns a SET head_turn_checkpoint_id = NULL, next_checkpoint_generation = NULL
WHERE a.id=$1 AND a.next_checkpoint_generation IS NOT NULL
AND a.settled_at<=$2::timestamptz-make_interval(secs=>$3)
AND NOT EXISTS (SELECT 1 FROM runner.turn_checkpoints p WHERE p.turn_id=a.id)
RETURNING a.id AS turn_id
