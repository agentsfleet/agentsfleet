-- A durable task's settlement replaced the run's answer with its notice
--. The settlement reads the runtime the report statement metered,
-- so it runs after that statement; the event, the session and an answer still
-- owed then carry the notice instead.
WITH event AS (
  UPDATE thread.turns SET answer_text = $3, updated_at = now()
  WHERE thread_id = $1 AND id = $2 AND opened_at IS NOT NULL
  RETURNING id
), session AS (
  UPDATE thread.threads SET resume_context = $4::jsonb, updated_at = now()
  WHERE id = $1
), owed AS (
  -- An answer still owed carries the notice instead, a failed run's
  -- unanswered notice included. A clean run that answered nothing owed none,
  -- so the notice is owed in its place, to the admission's reply destination,
  -- as the report statement owes a run's own text.
  INSERT INTO thread.deliveries AS o
    (id, fleet_id, workspace_id, provider, destination, turn_id, answer,
     receipt, claim_expires_at, delivered_at, attempt_count, created_at, updated_at)
  SELECT $5, a.thread_id, a.workspace_id, a.reply_provider, a.reply_address, a.id,
         $3, $6, clock_timestamp(), NULL, 0, now(), now()
  FROM thread.turns a
  WHERE a.thread_id = $1 AND a.id = $2 AND a.reply_provider = ANY($7::text[])
  ON CONFLICT ON CONSTRAINT uq_deliveries_fleet_id_turn_id
  DO UPDATE SET answer = EXCLUDED.answer, updated_at = now()
  WHERE o.delivered_at IS NULL
)
SELECT count(*) FROM event
