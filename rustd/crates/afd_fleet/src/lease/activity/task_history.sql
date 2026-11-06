-- Slot, lease and event ownership are rechecked in the write, not a preflight.
-- Cosmetic progress may be dropped under contention; never wait across ownership locks.
-- The existing event row serializes bounded progress updates. No new work queue.
WITH slot AS MATERIALIZED (
    SELECT s.fleet_id FROM runner.fleet_slots s
    WHERE s.fleet_id = $1::uuid AND s.last_runner_id = $5::uuid AND s.fencing_token = $6
      AND s.lease_expires_at > statement_timestamp()
    FOR UPDATE OF s SKIP LOCKED
), lease AS MATERIALIZED (
    SELECT l.fleet_event_id FROM runner.leases l JOIN slot s ON s.fleet_id = l.fleet_id
    WHERE l.id = $4::uuid AND l.runner_id = $5::uuid AND l.fencing_token = $6
      AND l.status = $7 AND l.lease_expires_at > statement_timestamp()
    FOR UPDATE OF l SKIP LOCKED
), held AS MATERIALIZED (
    SELECT e.id, e.task_activity, e.task_activity_sequence, e.task_activity_truncated
    FROM fleet.events e JOIN lease l ON l.fleet_event_id = e.id
    WHERE e.fleet_id = $1::uuid AND e.event_id = $2
    FOR UPDATE OF e SKIP LOCKED
), combined AS (
    SELECT item, COALESCE((item->'progress'->>'sequence')::bigint, ordinal::bigint) AS position
    FROM held, jsonb_array_elements(held.task_activity) WITH ORDINALITY AS old(item, ordinal)
    UNION ALL
    SELECT jsonb_set(item, '{progress,sequence}', to_jsonb(
        held.task_activity_sequence + ordinal::bigint)),
        held.task_activity_sequence + ordinal::bigint AS position
    FROM held, jsonb_array_elements($3::jsonb) WITH ORDINALITY AS incoming(item, ordinal)
), newest AS MATERIALIZED (
    SELECT DISTINCT ON (item->>'kind', item->'progress'->>'activity_id', item->'progress'->>'operation', item->'progress'->>'phase') item, position
    FROM combined
    ORDER BY item->>'kind', item->'progress'->>'activity_id', item->'progress'->>'operation', item->'progress'->>'phase',
        (item->'progress'->>'completed')::bigint DESC,
        COALESCE((item->'progress'->'bytes'->>'acknowledged')::bigint, 0) DESC,
        COALESCE((item->'progress'->'bytes'->>'reused')::bigint, 0) DESC, position DESC
), bounded AS (
    SELECT item, position, sum(octet_length(item::text) + 2) OVER (ORDER BY position DESC) AS bytes
    FROM newest ORDER BY position DESC LIMIT $8
), retained AS (
    SELECT item, position FROM bounded WHERE bytes <= $9
)
UPDATE fleet.events e SET
    task_activity = COALESCE((SELECT jsonb_agg(item ORDER BY position) FROM retained), jsonb_build_array()),
    task_activity_sequence = held.task_activity_sequence + jsonb_array_length($3::jsonb),
    task_activity_truncated = held.task_activity_truncated OR (SELECT count(*) FROM newest) > (SELECT count(*) FROM retained)
FROM held WHERE e.id = held.id
RETURNING jsonb_build_object('items', e.task_activity, 'truncated', e.task_activity_truncated)
