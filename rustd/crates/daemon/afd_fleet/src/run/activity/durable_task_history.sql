-- Thread, run and event ownership are rechecked in the write, not a preflight.
-- Cosmetic progress may be dropped under contention; never wait across ownership locks.
-- The existing event row serializes progress updates. No new work queue.
WITH held_thread AS MATERIALIZED (
    SELECT s.fleet_id FROM thread.threads s
    WHERE s.fleet_id = $1 AND s.last_runner_id = $5 AND s.fencing_token = $6
      AND s.run_expires_at > statement_timestamp()
    FOR NO KEY UPDATE OF s SKIP LOCKED
), run AS MATERIALIZED (
    SELECT l.turn_id FROM thread.runs l JOIN held_thread s ON s.fleet_id = l.fleet_id
    WHERE l.id = $4 AND l.runner_id = $5 AND l.fencing_token = $6
      AND l.status = $7 AND l.run_expires_at > statement_timestamp()
    FOR UPDATE OF l SKIP LOCKED
), held AS MATERIALIZED (
    SELECT e.id, e.durable_task_activity, e.durable_task_activity_sequence, e.durable_task_activity_truncated
    FROM thread.turns e JOIN run l ON l.turn_id = e.id
    WHERE e.thread_id = $1 AND e.id = $2 AND e.opened_at IS NOT NULL
    FOR UPDATE OF e SKIP LOCKED
), combined AS (
    SELECT item, COALESCE((item->'progress'->>'sequence')::bigint, ordinal::bigint) AS position
    FROM held, jsonb_array_elements(held.durable_task_activity) WITH ORDINALITY AS old(item, ordinal)
    UNION ALL
    SELECT jsonb_set(item, '{progress,sequence}', to_jsonb(
        held.durable_task_activity_sequence + ordinal::bigint)),
        held.durable_task_activity_sequence + ordinal::bigint AS position
    FROM held, jsonb_array_elements($3::jsonb) WITH ORDINALITY AS incoming(item, ordinal)
), newest AS MATERIALIZED (
    SELECT DISTINCT ON (item->>'kind', item->'progress'->>'activity_id', item->'progress'->>'operation', item->'progress'->>'phase') item, position
    FROM combined
    ORDER BY item->>'kind', item->'progress'->>'activity_id', item->'progress'->>'operation', item->'progress'->>'phase',
        (item->'progress'->>'completed')::bigint DESC,
        COALESCE((item->'progress'->'bytes'->>'acknowledged')::bigint, 0) DESC,
        COALESCE((item->'progress'->'bytes'->>'reused')::bigint, 0) DESC, position DESC
)
UPDATE thread.turns e SET
    durable_task_activity = COALESCE((SELECT jsonb_agg(item ORDER BY position) FROM newest), jsonb_build_array()),
    durable_task_activity_sequence = held.durable_task_activity_sequence + jsonb_array_length($3::jsonb),
    durable_task_activity_truncated = held.durable_task_activity_truncated
FROM held WHERE e.id = held.id
RETURNING jsonb_build_object('items', e.durable_task_activity, 'truncated', e.durable_task_activity_truncated)
