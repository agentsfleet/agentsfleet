WITH terminal AS MATERIALIZED (
    SELECT a.id FROM thread.turns a
    WHERE a.id=$3 AND a.next_checkpoint_generation IS NOT NULL
      AND a.settled_at<=$4::timestamptz-make_interval(secs=>$5)
      AND NOT EXISTS (SELECT 1 FROM runner.checkpoints p WHERE p.turn_id=a.id)
), expired AS MATERIALIZED (
    SELECT r.operation_id FROM thread.replay_receipts r CROSS JOIN terminal
    WHERE r.scope=$1 ORDER BY r.created_at,r.operation_id LIMIT $2
)
DELETE FROM thread.replay_receipts r USING expired e
WHERE r.scope=$1 AND r.operation_id=e.operation_id
