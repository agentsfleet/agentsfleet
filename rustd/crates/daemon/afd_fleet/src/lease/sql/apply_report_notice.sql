-- A durable task's settlement replaced the run's answer with its notice
--. The settlement reads the runtime the report statement metered,
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
  -- An answer still owed carries the notice instead. A run that answered
  -- nothing owed none, so the notice is owed in its place, to the admission's
  -- reply destination, as the report statement owes a run's own text.
  INSERT INTO fleet.obligations AS o
    (id, fleet_id, workspace_id, provider, destination, event_id, answer,
     receipt, claim_expires_at, delivered_at, attempt_count, created_at, updated_at)
  SELECT $5, a.fleet_id, a.workspace_id, a.reply_provider, a.reply_address, a.id,
         $3, $6, clock_timestamp(), NULL, 0, now(), now()
  FROM fleet.admissions a
  WHERE a.fleet_id = $1 AND a.id = $2 AND a.reply_provider = ANY($7::text[])
  ON CONFLICT ON CONSTRAINT uq_obligations_fleet_id_event_id
  DO UPDATE SET answer = EXCLUDED.answer, updated_at = now()
  WHERE o.delivered_at IS NULL
)
SELECT count(*) FROM event
