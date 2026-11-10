-- A durable task's settlement replaced the run's answer with its notice
-- (M245 §6). The settlement reads the runtime the report statement metered,
-- so it runs after that statement; the event, the session and an answer still
-- owed then carry the notice instead.
WITH event AS (
  UPDATE fleet.events SET response_text = $3, updated_at = now()
  WHERE fleet_id = $1 AND event_id = $2
  RETURNING event_id
), session AS (
  UPDATE fleet.sessions SET context_json = $4::jsonb, updated_at = now()
  WHERE fleet_id = $1
), owed AS (
  UPDATE fleet.obligations SET answer = $3, updated_at = now()
  WHERE fleet_id = $1 AND event_id = $2 AND delivered_at IS NULL
)
SELECT count(*) FROM event
