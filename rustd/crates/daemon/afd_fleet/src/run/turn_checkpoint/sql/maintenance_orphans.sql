WITH expired AS MATERIALIZED (
    SELECT op.turn_id,op.operation_id FROM runner.turn_checkpoint_operations op
    WHERE op.created_at<=statement_timestamp()-make_interval(secs=>$1)
      AND NOT EXISTS (SELECT 1 FROM thread.turns a WHERE a.id=op.turn_id
          AND (a.settled_at IS NULL OR a.settled_at>statement_timestamp()-make_interval(secs=>$1)))
    ORDER BY op.created_at,op.turn_id,op.operation_id LIMIT $2
), fresh AS MATERIALIZED (SELECT clock_timestamp() AS now FROM expired LIMIT 1)
DELETE FROM runner.turn_checkpoint_operations op USING expired e, fresh
WHERE op.turn_id=e.turn_id AND op.operation_id=e.operation_id
  AND op.created_at<=fresh.now-make_interval(secs=>$1)
  AND NOT EXISTS (SELECT 1 FROM thread.turns a WHERE a.id=op.turn_id
      AND (a.settled_at IS NULL OR a.settled_at>fresh.now-make_interval(secs=>$1)))
