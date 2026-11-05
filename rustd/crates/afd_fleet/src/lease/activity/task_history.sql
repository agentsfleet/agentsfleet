-- The existing event row serializes bounded progress updates. No new work queue.
WITH held AS MATERIALIZED (
    SELECT id, task_activity FROM fleet.events
    WHERE fleet_id = $1::uuid AND event_id = $2 FOR UPDATE
), combined AS (
    SELECT item, ordinal::bigint AS position
    FROM held, jsonb_array_elements(held.task_activity->'items') WITH ORDINALITY AS old(item, ordinal)
    UNION ALL
    SELECT item, 128 + ordinal::bigint AS position
    FROM jsonb_array_elements($3::jsonb) WITH ORDINALITY AS incoming(item, ordinal)
), newest AS MATERIALIZED (
    SELECT DISTINCT ON (item->>'kind', item->'progress'->>'activity_id', item->'progress'->>'operation', item->'progress'->>'phase') item, position
    FROM combined
    ORDER BY item->>'kind', item->'progress'->>'activity_id', item->'progress'->>'operation', item->'progress'->>'phase', position DESC
), bounded AS (
    SELECT item, position, sum(octet_length(item::text) + 2) OVER (ORDER BY position DESC) AS bytes
    FROM newest ORDER BY position DESC LIMIT 128
), retained AS (
    SELECT item, position FROM bounded WHERE bytes <= 120000
)
UPDATE fleet.events e SET task_activity = jsonb_build_object(
    'items', COALESCE((SELECT jsonb_agg(item ORDER BY position) FROM retained), '[]'::jsonb),
    'truncated', (held.task_activity->>'truncated')::boolean OR (SELECT count(*) FROM newest) > (SELECT count(*) FROM retained))
FROM held WHERE e.id = held.id
